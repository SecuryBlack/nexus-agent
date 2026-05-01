use opentelemetry_proto::tonic::collector::metrics::v1::{
    metrics_service_server::{MetricsService, MetricsServiceServer},
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use prost::Message;
use tonic::{Request, Response, Status};

use crate::proto::{
    tunnel_envelope::Payload,
    TunnelEnvelope,
};

/// Servicio proxy OTLP que recibe métricas localmente y las reenvía al bridge.
pub struct ProxyService {
    bridge_tx: tokio::sync::mpsc::Sender<TunnelEnvelope>,
}

impl ProxyService {
    pub fn new(bridge_tx: tokio::sync::mpsc::Sender<TunnelEnvelope>) -> Self {
        Self { bridge_tx }
    }
}

#[tonic::async_trait]
impl MetricsService for ProxyService {
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        let payload = request.into_inner().encode_to_vec();

        let envelope = TunnelEnvelope {
            payload: Some(Payload::OtlpMetricsPayload(payload)),
        };

        self.bridge_tx
            .send(envelope)
            .await
            .map_err(|_| Status::internal("bridge channel closed"))?;

        Ok(Response::new(ExportMetricsServiceResponse::default()))
    }
}

/// Arranca el servidor proxy OTLP en `127.0.0.1:4317`.
pub async fn run_proxy(
    bridge_tx: tokio::sync::mpsc::Sender<TunnelEnvelope>,
) -> anyhow::Result<()> {
    let addr = "127.0.0.1:4317".parse()?;
    let service = ProxyService::new(bridge_tx);

    tracing::info!("OTLP proxy listening on {}", addr);

    tonic::transport::Server::builder()
        .add_service(MetricsServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
