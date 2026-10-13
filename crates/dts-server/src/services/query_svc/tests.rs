// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::pin::Pin;
use std::sync::Arc;

use arrow_array::{ArrayRef, Int32Array, RecordBatch, StringArray};
use arrow_flight::encode::FlightDataEncoderBuilder;
use arrow_flight::flight_service_server::{FlightService, FlightServiceServer};
use arrow_flight::sql::server::FlightSqlService;
use arrow_flight::sql::{CommandStatementQuery, ProstMessageExt, SqlInfo, TicketStatementQuery};
use arrow_flight::{FlightDescriptor, FlightEndpoint, FlightInfo, Ticket};
use arrow_schema::{DataType, Field, Schema};
use futures::{Stream, StreamExt, TryStreamExt};
use prost::Message;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

use super::*;

struct FlightFixture {
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
    delay: Duration,
    batch_delay: Duration,
    fail: bool,
}

#[tonic::async_trait]
impl FlightSqlService for FlightFixture {
    type FlightService = Self;

    async fn get_flight_info_statement(
        &self,
        _query: CommandStatementQuery,
        _request: Request<FlightDescriptor>,
    ) -> Result<Response<FlightInfo>, Status> {
        tokio::time::sleep(self.delay).await;
        if self.fail {
            return Err(Status::invalid_argument(
                "SQL synthetic_private_value /private/database",
            ));
        }
        let ticket = TicketStatementQuery {
            statement_handle: b"fixture".to_vec().into(),
        };
        let info = FlightInfo::new()
            .try_with_schema(&self.schema)
            .map_err(|_| Status::internal("fixture schema failed"))?
            .with_endpoint(
                FlightEndpoint::new().with_ticket(Ticket::new(ticket.as_any().encode_to_vec())),
            );
        Ok(Response::new(info))
    }

    async fn do_get_statement(
        &self,
        _ticket: TicketStatementQuery,
        _request: Request<Ticket>,
    ) -> Result<Response<<Self as FlightService>::DoGetStream>, Status> {
        let delay = self.batch_delay;
        let stream = FlightDataEncoderBuilder::new()
            .with_schema(self.schema.clone())
            .build(
                futures::stream::iter(self.batches.clone()).then(move |batch| async move {
                    tokio::time::sleep(delay).await;
                    Ok(batch)
                }),
            )
            .map_err(|_| Status::internal("fixture encoding failed"));
        let stream: Pin<Box<dyn Stream<Item = Result<arrow_flight::FlightData, Status>> + Send>> =
            Box::pin(stream);
        Ok(Response::new(stream))
    }

    async fn register_sql_info(&self, _id: i32, _result: &SqlInfo) {}
}

struct TestServer {
    service: QueryService,
    task: JoinHandle<Result<(), tonic::transport::Error>>,
}

impl TestServer {
    async fn start(fixture: FlightFixture, limits: QueryLimits) -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let task = tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(FlightServiceServer::new(fixture))
                .serve_with_incoming(TcpListenerStream::new(listener)),
        );
        let config = SwanLakeConfig {
            enabled: true,
            host: "127.0.0.1".into(),
            port,
            ..SwanLakeConfig::default()
        };
        let mut service = QueryService::new(&config);
        service.limits = limits;
        Ok(Self { service, task })
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn integer_fixture(batch_count: usize, rows: usize) -> anyhow::Result<FlightFixture> {
    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int32, false)]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(Int32Array::from(vec![1; rows])) as ArrayRef],
    )?;
    Ok(FlightFixture {
        schema,
        batches: vec![batch; batch_count],
        delay: Duration::ZERO,
        batch_delay: Duration::ZERO,
        fail: false,
    })
}

#[tokio::test]
async fn flight_query_preserves_empty_schema() -> anyhow::Result<()> {
    let server = TestServer::start(integer_fixture(0, 0)?, QueryLimits::default()).await?;
    let result = server
        .service
        .execute("SELECT id FROM events WHERE false")
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    assert_eq!(result.row_count, 0);
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0].name, "id");
    Ok(())
}

#[tokio::test]
async fn flight_query_applies_row_limit_across_batches() -> anyhow::Result<()> {
    let limits = QueryLimits {
        max_rows: 3,
        ..QueryLimits::default()
    };
    let server = TestServer::start(integer_fixture(2, 2)?, limits).await?;
    let result = server.service.execute("SELECT id FROM events").await;
    assert!(matches!(result, Err(AppError::BadRequest(message)) if message.contains("超过 3 行")));
    Ok(())
}

#[tokio::test]
async fn flight_query_applies_byte_limit_across_batches() -> anyhow::Result<()> {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Utf8,
        false,
    )]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(StringArray::from(vec!["x".repeat(100)])) as ArrayRef],
    )?;
    let fixture = FlightFixture {
        schema,
        batches: vec![batch.clone(), batch],
        delay: Duration::ZERO,
        batch_delay: Duration::ZERO,
        fail: false,
    };
    let limits = QueryLimits {
        max_json_bytes: 250,
        ..QueryLimits::default()
    };
    let server = TestServer::start(fixture, limits).await?;
    let result = server.service.execute("SELECT value FROM events").await;
    assert!(matches!(result, Err(AppError::BadRequest(message)) if message.contains("JSON")));
    Ok(())
}

#[tokio::test]
async fn flight_query_applies_deadline_and_redacts_engine_errors() -> anyhow::Result<()> {
    let mut fixture = integer_fixture(1, 1)?;
    fixture.delay = Duration::from_millis(200);
    let limits = QueryLimits {
        timeout: Duration::from_millis(30),
        ..QueryLimits::default()
    };
    let server = TestServer::start(fixture, limits).await?;
    let result = server.service.execute("SELECT id FROM events").await;
    assert!(
        matches!(&result, Err(AppError::BadRequest(message)) if message.contains("超时")),
        "Expected the end-to-end deadline error, got {result:?}"
    );

    let mut fixture = integer_fixture(1, 1)?;
    fixture.fail = true;
    let server = TestServer::start(fixture, QueryLimits::default()).await?;
    let result = server
        .service
        .execute("SELECT 'synthetic_private_value'")
        .await;
    let Err(error) = result else {
        anyhow::bail!("expected engine failure")
    };
    assert!(!error.to_string().contains("synthetic_private_value"));
    assert!(!error.to_string().contains("/private/database"));
    Ok(())
}

#[tokio::test]
async fn flight_query_applies_deadline_while_streaming_results() -> anyhow::Result<()> {
    let mut fixture = integer_fixture(1, 1)?;
    fixture.batch_delay = Duration::from_millis(200);
    let limits = QueryLimits {
        timeout: Duration::from_millis(30),
        ..QueryLimits::default()
    };
    let server = TestServer::start(fixture, limits).await?;
    let result = server.service.execute("SELECT id FROM events").await;
    assert!(
        matches!(&result, Err(AppError::BadRequest(message)) if message.contains("超时")),
        "The deadline must include reading result batches, got {result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn flight_schema_listing_keeps_catalogs_separate() -> anyhow::Result<()> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("table_catalog", DataType::Utf8, false),
        Field::new("table_schema", DataType::Utf8, false),
        Field::new("table_name", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(vec!["warehouse_a", "warehouse_b"])) as ArrayRef,
            Arc::new(StringArray::from(vec!["main", "main"])) as ArrayRef,
            Arc::new(StringArray::from(vec!["orders", "customers"])) as ArrayRef,
        ],
    )?;
    let fixture = FlightFixture {
        schema,
        batches: vec![batch],
        delay: Duration::ZERO,
        batch_delay: Duration::ZERO,
        fail: false,
    };
    let server = TestServer::start(fixture, QueryLimits::default()).await?;
    let result = server
        .service
        .list_schemas()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].catalog.as_deref(), Some("warehouse_a"));
    assert_eq!(result[0].tables[0].name, "orders");
    assert_eq!(result[1].catalog.as_deref(), Some("warehouse_b"));
    assert_eq!(result[1].tables[0].name, "customers");
    Ok(())
}

#[test]
fn accepts_one_read_only_statement_with_ctes_and_quoted_semicolons() {
    for sql in [
        "SELECT 1",
        "VALUES (1), (2)",
        "WITH s AS (SELECT id FROM main.events) SELECT COUNT(*) FROM s",
        "SELECT ';DELETE FROM events;' AS literal",
        "SELECT id FROM events UNION ALL SELECT id FROM events_old",
        "SELECT SUM(amount) OVER (PARTITION BY category) FROM orders",
    ] {
        assert!(validate_read_only_sql(sql).is_ok(), "{sql}");
    }
}

#[test]
fn rejects_writes_multiple_statements_and_external_access() {
    for sql in [
        "",
        "SELECT 1; DROP TABLE events",
        "UPDATE events SET id = 1",
        "DELETE FROM events",
        "CREATE TABLE events AS SELECT 1",
        "ATTACH 'other.db' AS other",
        "COPY (SELECT 1) TO '/tmp/data.csv'",
        "INSTALL httpfs",
        "LOAD httpfs",
        "SELECT 1 INTO events",
        "SELECT * FROM read_parquet('/tmp/data.parquet')",
        "SELECT * FROM '/tmp/data.csv'",
        "SELECT * FROM \"data.csv\"",
        "SELECT read_text('/private/file')",
        "SELECT nextval('seq')",
        "SELECT (SELECT read_blob('/private/file'))",
    ] {
        assert!(validate_read_only_sql(sql).is_err(), "{sql}");
    }
}
