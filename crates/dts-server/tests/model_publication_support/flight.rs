use arrow_array::{ArrayRef, RecordBatch, StringArray};
use arrow_flight::{
    FlightDescriptor, FlightEndpoint, FlightInfo, Ticket,
    encode::FlightDataEncoderBuilder,
    flight_service_server::{FlightService, FlightServiceServer},
    sql::{
        CommandStatementQuery, CommandStatementUpdate, ProstMessageExt, SqlInfo,
        TicketStatementQuery,
        server::{FlightSqlService, PeekableFlightDataStream},
    },
};
use arrow_schema::{DataType, Field, Schema};
use dts_server::{config::SwanLakeConfig, services::query_svc::QueryService};
use futures::{Stream, TryStreamExt};
use prost::Message;
use std::{
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::{net::TcpListener, sync::Notify, task::JoinHandle};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

#[derive(Default)]
pub struct Observations {
    pub created: AtomicBool,
    pub create_calls: AtomicUsize,
    pub sql: Mutex<Vec<String>>,
    pub fail_next_columns: AtomicBool,
    pub wrong_type: AtomicBool,
    pub pause_columns: AtomicBool,
    pub columns_started: Notify,
    pub release_columns: Notify,
}

// This fixture implements the actual Flight SQL protocol and controlled
// information_schema observations. It is neither DuckDB nor a SwanLake POC.
struct ProtocolFixture {
    observations: Arc<Observations>,
}

#[tonic::async_trait]
impl FlightSqlService for ProtocolFixture {
    type FlightService = Self;

    async fn do_put_statement_update(
        &self,
        command: CommandStatementUpdate,
        _request: Request<PeekableFlightDataStream>,
    ) -> Result<i64, Status> {
        if command.query != "CREATE TABLE \"main\".\"orders\" (\n  \"id\" BIGINT NOT NULL\n);" {
            return Err(Status::invalid_argument("unexpected publication DDL"));
        }
        self.observations.sql.lock().unwrap().push(command.query);
        self.observations
            .create_calls
            .fetch_add(1, Ordering::SeqCst);
        if self.observations.created.swap(true, Ordering::SeqCst) {
            return Err(Status::already_exists("duplicate physical CREATE"));
        }
        Ok(0)
    }

    async fn get_flight_info_statement(
        &self,
        command: CommandStatementQuery,
        _request: Request<FlightDescriptor>,
    ) -> Result<Response<FlightInfo>, Status> {
        self.observations
            .sql
            .lock()
            .unwrap()
            .push(command.query.clone());
        let kind = if command.query.contains("FROM information_schema.tables ") {
            if !command.query.contains("table_catalog=current_database()")
                || !command
                    .query
                    .contains("table_schema='main' AND table_name='orders'")
            {
                return Err(Status::invalid_argument("unbound table lookup"));
            }
            "tables"
        } else if command.query.contains("FROM information_schema.columns ") {
            if !command
                .query
                .contains("table_catalog='lake' AND table_schema='main' AND table_name='orders'")
            {
                return Err(Status::invalid_argument("unbound column lookup"));
            }
            if self
                .observations
                .fail_next_columns
                .swap(false, Ordering::SeqCst)
            {
                return Err(Status::unavailable("controlled verification outage"));
            }
            if self.observations.pause_columns.load(Ordering::SeqCst) {
                self.observations.columns_started.notify_one();
                self.observations.release_columns.notified().await;
            }
            "columns"
        } else if command
            .query
            .contains("FROM information_schema.table_constraints ")
        {
            if !command.query.contains(
                "c.table_catalog='lake' AND c.table_schema='main' AND c.table_name='orders'",
            ) {
                return Err(Status::invalid_argument("unbound primary key lookup"));
            }
            "keys"
        } else {
            return Err(Status::invalid_argument("unexpected publication read"));
        };
        let batch = self.batch(kind)?;
        let ticket = TicketStatementQuery {
            statement_handle: kind.as_bytes().to_vec().into(),
        };
        Ok(Response::new(
            FlightInfo::new()
                .try_with_schema(batch.schema().as_ref())
                .map_err(|_| Status::internal("fixture schema failed"))?
                .with_endpoint(
                    FlightEndpoint::new().with_ticket(Ticket::new(ticket.as_any().encode_to_vec())),
                ),
        ))
    }

    async fn do_get_statement(
        &self,
        ticket: TicketStatementQuery,
        _request: Request<Ticket>,
    ) -> Result<Response<<Self as FlightService>::DoGetStream>, Status> {
        let kind = std::str::from_utf8(ticket.statement_handle.as_ref())
            .map_err(|_| Status::invalid_argument("invalid fixture ticket"))?;
        let batch = self.batch(kind)?;
        let stream = FlightDataEncoderBuilder::new()
            .with_schema(batch.schema())
            .build(futures::stream::iter(vec![Ok(batch)]))
            .map_err(|_| Status::internal("fixture encoding failed"));
        let stream: Pin<Box<dyn Stream<Item = Result<arrow_flight::FlightData, Status>> + Send>> =
            Box::pin(stream);
        Ok(Response::new(stream))
    }

    async fn register_sql_info(&self, _id: i32, _result: &SqlInfo) {}
}

impl ProtocolFixture {
    fn batch(&self, kind: &str) -> Result<RecordBatch, Status> {
        let created = self.observations.created.load(Ordering::SeqCst);
        let values = match kind {
            "tables" => vec![
                ("table_catalog", if created { vec!["lake"] } else { vec![] }),
                ("table_schema", if created { vec!["main"] } else { vec![] }),
                ("table_name", if created { vec!["orders"] } else { vec![] }),
                (
                    "table_type",
                    if created { vec!["BASE TABLE"] } else { vec![] },
                ),
            ],
            "columns" => vec![
                ("column_name", vec!["id"]),
                (
                    "data_type",
                    vec![if self.observations.wrong_type.load(Ordering::SeqCst) {
                        "VARCHAR"
                    } else {
                        "BIGINT"
                    }],
                ),
                ("is_nullable", vec!["NO"]),
            ],
            "keys" => vec![("column_name", vec![])],
            _ => return Err(Status::invalid_argument("unknown fixture ticket")),
        };
        string_batch(values).map_err(|_| Status::internal("fixture batch failed"))
    }
}

fn string_batch(values: Vec<(&str, Vec<&str>)>) -> Result<RecordBatch, arrow_schema::ArrowError> {
    let fields = values
        .iter()
        .map(|(name, _)| Field::new(*name, DataType::Utf8, false));
    let schema = Arc::new(Schema::new(fields.collect::<Vec<_>>()));
    let arrays = values
        .into_iter()
        .map(|(_, values)| Arc::new(StringArray::from(values)) as ArrayRef)
        .collect();
    RecordBatch::try_new(schema, arrays)
}

pub struct TestFlight {
    pub query: QueryService,
    pub observations: Arc<Observations>,
    task: JoinHandle<Result<(), tonic::transport::Error>>,
}

impl TestFlight {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let observations = Arc::new(Observations::default());
        let task = tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(FlightServiceServer::new(ProtocolFixture {
                    observations: observations.clone(),
                }))
                .serve_with_incoming(TcpListenerStream::new(listener)),
        );
        Self {
            query: QueryService::new(&SwanLakeConfig {
                enabled: true,
                host: "127.0.0.1".into(),
                port,
                ..SwanLakeConfig::default()
            }),
            observations,
            task,
        }
    }
}

impl Drop for TestFlight {
    fn drop(&mut self) {
        self.task.abort();
    }
}
