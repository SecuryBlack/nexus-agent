use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::time::{interval, sleep};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic::Request;

use crate::proto::{
    tunnel_service_client::TunnelServiceClient,
    tunnel_envelope::Payload,
    ClientHello, Heartbeat, ServerHello, TunnelEnvelope,
};

/// Cliente del túnel bidireccional hacia SecuryBlack Edge Gateway.
pub struct TunnelClient {
    endpoint: String,
    token: String,
}

impl TunnelClient {
    pub fn new(endpoint: String, token: String) -> Self {
        Self { endpoint, token }
    }

    /// Bucle principal: conecta, hace handshake, y mantiene heartbeats.
    /// Reconecta automáticamente con backoff exponencial si la conexión cae.
    pub async fn run(&self) {
        let mut backoff_secs = 1u64;
        loop {
            match self.connect_and_stream().await {
                Ok(()) => {
                    tracing::info!("tunnel closed gracefully, reconnecting…");
                    backoff_secs = 1;
                }
                Err(e) => {
                    tracing::error!("tunnel error: {}, reconnecting in {}s", e, backoff_secs);
                    sleep(Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(60);
                }
            }
        }
    }

    async fn connect_and_stream(&self) -> anyhow::Result<()> {
        let channel = Channel::from_shared(self.endpoint.clone())?
            .connect()
            .await?;

        let mut client = TunnelServiceClient::new(channel);

        let (tx, rx) = mpsc::channel::<TunnelEnvelope>(128);

        // ─── Handshake: enviar ClientHello ─────────────────────────────────
        let hello_msg = TunnelEnvelope {
            payload: Some(Payload::Hello(ClientHello {
                agent_id: String::new(), // se asignará en el gateway vía token
                token: self.token.clone(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                local_agents: vec![],
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                hostname: hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "unknown".to_string()),
            })),
        };
        tx.send(hello_msg).await?;

        let outbound = ReceiverStream::new(rx);
        let response = client.stream(Request::new(outbound)).await?;
        let mut inbound = response.into_inner();

        // ─── Esperar ServerHello ───────────────────────────────────────────
        let server_hello = inbound
            .message()
            .await?
            .ok_or_else(|| anyhow::anyhow!("stream closed before ServerHello"))?;

        match server_hello.payload {
            Some(Payload::ServerHello(ServerHello { accepted: true, session_id, .. })) => {
                tracing::info!(session_id = %session_id, "tunnel handshake accepted");
            }
            Some(Payload::ServerHello(ServerHello { accepted: false, reason, .. })) => {
                anyhow::bail!("handshake rejected: {}", reason);
            }
            other => {
                anyhow::bail!("unexpected first message: {:?}", other);
            }
        }

        // ─── Loop de heartbeat ─────────────────────────────────────────────
        let mut ticker = interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;

            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;

            let hb = TunnelEnvelope {
                payload: Some(Payload::Heartbeat(Heartbeat { timestamp_ms: ts })),
            };

            if tx.send(hb).await.is_err() {
                tracing::warn!("tunnel send channel closed, ending stream loop");
                return Ok(());
            }
        }
    }
}
