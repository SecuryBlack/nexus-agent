use opentelemetry_proto::tonic::collector::metrics::v1::{
    metrics_service_server::{MetricsService, MetricsServiceServer},
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use prost::Message;
use tonic::{Request, Response, Status};

use crate::proto::security_service_server::{SecurityService, SecurityServiceServer};
use crate::proto::{
    tunnel_envelope::Payload,
    SecurityEventRequest, SecurityEventResponse, TunnelEnvelope,
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

/// Servicio proxy para eventos de seguridad que recibe alertas localmente de ferro-sentry y las reenvía al bridge.
pub struct SecurityProxyService {
    bridge_tx: tokio::sync::mpsc::Sender<TunnelEnvelope>,
}

impl SecurityProxyService {
    pub fn new(bridge_tx: tokio::sync::mpsc::Sender<TunnelEnvelope>) -> Self {
        Self { bridge_tx }
    }
}

#[tonic::async_trait]
impl SecurityService for SecurityProxyService {
    async fn send_event(
        &self,
        request: Request<SecurityEventRequest>,
    ) -> Result<Response<SecurityEventResponse>, Status> {
        let req = request.into_inner();
        let payload = req.event_json.into_bytes();

        let envelope = TunnelEnvelope {
            payload: Some(Payload::SecurityEventPayload(payload)),
        };

        self.bridge_tx
            .send(envelope)
            .await
            .map_err(|_| Status::internal("bridge channel closed"))?;

        Ok(Response::new(SecurityEventResponse { success: true }))
    }
}

/// Arranca el servidor proxy OTLP y Seguridad en `127.0.0.1:4317`.
pub async fn run_proxy(
    bridge_tx: tokio::sync::mpsc::Sender<TunnelEnvelope>,
) -> anyhow::Result<()> {
    let addr = "127.0.0.1:4317".parse()?;
    
    let metrics_service = ProxyService::new(bridge_tx.clone());
    let security_service = SecurityProxyService::new(bridge_tx);

    tracing::info!("Local gRPC proxy listening on {}", addr);

    tonic::transport::Server::builder()
        .add_service(MetricsServiceServer::new(metrics_service))
        .add_service(SecurityServiceServer::new(security_service))
        .serve(addr)
        .await?;

    Ok(())
}
