use arrow_array::{ArrayRef, Int64Array, RecordBatch, StringArray};
use arrow_flight::encode::FlightDataEncoderBuilder;
use arrow_flight::flight_service_server::{FlightService, FlightServiceServer};
use arrow_flight::sql::server::FlightSqlService;
use arrow_flight::sql::{CommandStatementQuery, ProstMessageExt, SqlInfo, TicketStatementQuery};
use arrow_flight::{FlightDescriptor, FlightEndpoint, FlightInfo, Ticket};
use arrow_schema::{DataType, Field, Schema};
use dts_server::{config::SwanLakeConfig, services::query_svc::QueryService};
use futures::{Stream, TryStreamExt};
use prost::Message;
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::{net::TcpListener, sync::Notify, task::JoinHandle};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

#[derive(Default)]
pub struct Observations {
    pub data_calls: AtomicUsize,
    pub paused: AtomicBool,
    pub started: Notify,
    pub released: Notify,
}

// Controlled protocol fixture: it supplies actual Arrow Flight observations to
// the production client; it is not a SQL engine or a SwanLake POC.
struct ProtocolFixture {
    observations: Arc<Observations>,
    metadata: RecordBatch,
    data: RecordBatch,
}
#[tonic::async_trait]
impl FlightSqlService for ProtocolFixture {
    type FlightService = Self;
    async fn get_flight_info_statement(
        &self,
        query: CommandStatementQuery,
        _request: Request<FlightDescriptor>,
    ) -> Result<Response<FlightInfo>, Status> {
        let metadata = query.query.contains("information_schema");
        if !metadata {
            if !query.query.contains("orders") {
                return Err(Status::invalid_argument("unexpected test SQL"));
            }
            self.observations.data_calls.fetch_add(1, Ordering::SeqCst);
        }
        let batch = if metadata { &self.metadata } else { &self.data };
        let ticket = TicketStatementQuery {
            statement_handle: if metadata {
                b"metadata".to_vec().into()
            } else {
                b"data".to_vec().into()
            },
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
        let metadata = ticket.statement_handle.as_ref() == b"metadata";
        if !metadata && self.observations.paused.load(Ordering::SeqCst) {
            self.observations.started.notify_one();
            self.observations.released.notified().await;
        }
        let batch = if metadata {
            self.metadata.clone()
        } else {
            self.data.clone()
        };
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

pub struct TestFlight {
    pub query: QueryService,
    pub observations: Arc<Observations>,
    task: JoinHandle<Result<(), tonic::transport::Error>>,
}
impl TestFlight {
    pub async fn start() -> Self {
        let observations = Arc::new(Observations::default());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(FlightServiceServer::new(ProtocolFixture {
                    observations: observations.clone(),
                    metadata: metadata_batch(),
                    data: data_batch(),
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

fn metadata_batch() -> RecordBatch {
    let mut fields = Vec::new();
    let mut values: Vec<ArrayRef> = Vec::new();
    for (name, contents) in [
        ("table_catalog", vec!["lake", "lake"]),
        ("schema_name", vec!["main", "main"]),
        ("table_name", vec!["orders", "orders"]),
        ("table_type", vec!["BASE TABLE", "BASE TABLE"]),
        ("column_name", vec!["region", "amount"]),
        ("data_type", vec!["VARCHAR", "BIGINT"]),
        ("is_nullable", vec!["NO", "NO"]),
    ] {
        fields.push(Field::new(name, DataType::Utf8, false));
        values.push(Arc::new(StringArray::from(contents)));
    }
    fields.push(Field::new("ordinal_position", DataType::Int64, false));
    values.push(Arc::new(Int64Array::from(vec![1, 2])));
    RecordBatch::try_new(Arc::new(Schema::new(fields)), values).unwrap()
}
fn data_batch() -> RecordBatch {
    RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("region", DataType::Utf8, false),
            Field::new("amount", DataType::Int64, false),
        ])),
        vec![
            Arc::new(StringArray::from(vec!["华东", "华南"])) as ArrayRef,
            Arc::new(Int64Array::from(vec![123456789, 987654321])) as ArrayRef,
        ],
    )
    .unwrap()
}
