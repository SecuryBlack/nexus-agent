//! Túnel HTTP: une un flujo `TunnelService.Data` con un puerto local.
//!
//! El gateway manda un `TunnelOpen` por el stream de control cuando un
//! visitante necesita una conexión nueva. Aquí se conecta a
//! `127.0.0.1:<port>` y se abre la llamada `Data` por la misma conexión HTTP/2
//! del túnel, que copia bytes en los dos sentidos hasta que un lado cierra.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;
use tonic::transport::Channel;

use crate::proto::{DataChunk, TunnelOpen, tunnel_service_client::TunnelServiceClient};

const STREAM_ID_HEADER: &str = "x-sb-stream-id";

/// Atiende un `TunnelOpen`. Solo conecta a puertos de `allowed_ports`
/// (`tunnel_ports` en la config local): el gateway no decide por sí solo qué
/// se expone de esta máquina.
pub async fn serve(
    mut client: TunnelServiceClient<Channel>,
    open: TunnelOpen,
    allowed_ports: &[u16],
) -> anyhow::Result<()> {
    let port = u16::try_from(open.port)
        .ok()
        .filter(|p| allowed_ports.contains(p))
        .ok_or_else(|| anyhow::anyhow!("port {} is not in tunnel_ports", open.port))?;

    // Si el puerto local no responde no se abre el flujo: el gateway agota su
    // espera y responde 502 al visitante.
    let local = TcpStream::connect(("127.0.0.1", port)).await?;
    let (mut local_rd, mut local_wr) = local.into_split();

    // Canal corto: si el gateway no drena, se deja de leer del puerto local.
    let (tx, rx) = mpsc::channel::<DataChunk>(16);
    let mut request = Request::new(ReceiverStream::new(rx));
    request
        .metadata_mut()
        .insert(STREAM_ID_HEADER, open.stream_id.parse()?);
    let mut inbound = client.data(request).await?.into_inner();

    // `move`: al acabar, suelta `tx` y con eso cierra nuestro lado del flujo
    // una vez enviado lo que quedaba en el canal.
    let upstream = async move {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            let n = local_rd.read(&mut buf).await?;
            if n == 0 {
                return anyhow::Ok(());
            }
            let data = buf[..n].to_vec();
            if tx.send(DataChunk { data }).await.is_err() {
                return Ok(());
            }
        }
    };
    let downstream = async {
        while let Some(chunk) = inbound.message().await? {
            local_wr.write_all(&chunk.data).await?;
        }
        anyhow::Ok(())
    };
    tokio::pin!(downstream);

    tokio::select! {
        // El puerto local cerró (p. ej. una respuesta HTTP/1.0). No se suelta
        // `inbound` todavía: eso cancelaría la llamada y el gateway tiraría lo
        // que aún no ha leído. Se espera a que cierre él, que lo hace al ver
        // nuestro fin de flujo.
        r = upstream => {
            r?;
            downstream.await
        }
        // El gateway cerró: ya no hay quien lea, se corta todo.
        r = &mut downstream => r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::TunnelEnvelope;
    use crate::proto::tunnel_service_server::{TunnelService, TunnelServiceServer};
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tonic::{Response, Status, Streaming};

    /// Gateway simulado: por `Data` manda "ping" y devuelve al test todo lo
    /// que conteste el agente hasta que este cierra su lado.
    struct FakeGateway {
        got: mpsc::Sender<(String, Vec<u8>)>,
    }

    #[tonic::async_trait]
    impl TunnelService for FakeGateway {
        type StreamStream = ReceiverStream<Result<TunnelEnvelope, Status>>;
        type DataStream = ReceiverStream<Result<DataChunk, Status>>;

        async fn stream(
            &self,
            _: Request<Streaming<TunnelEnvelope>>,
        ) -> Result<Response<Self::StreamStream>, Status> {
            Err(Status::unimplemented("not used in this test"))
        }

        async fn data(
            &self,
            request: Request<Streaming<DataChunk>>,
        ) -> Result<Response<Self::DataStream>, Status> {
            let id = request
                .metadata()
                .get(STREAM_ID_HEADER)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            let mut inbound = request.into_inner();
            let (tx, rx) = mpsc::channel(4);
            let got = self.got.clone();
            tokio::spawn(async move {
                let _ = tx
                    .send(Ok(DataChunk {
                        data: b"ping".to_vec(),
                    }))
                    .await;
                let mut all = Vec::new();
                while let Ok(Some(chunk)) = inbound.message().await {
                    all.extend_from_slice(&chunk.data);
                }
                let _ = got.send((id, all)).await;
                // tx se suelta aquí: el gateway cierra su lado.
            });
            Ok(Response::new(ReceiverStream::new(rx)))
        }
    }

    #[tokio::test]
    async fn pipes_a_data_stream_to_an_allowed_local_port() {
        // App local: lee "ping", contesta 2 MB y cierra sin esperar, como un
        // servidor HTTP/1.0. Todo lo escrito antes del cierre tiene que llegar.
        const REPLY_LEN: usize = 2 * 1024 * 1024;
        let app = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let app_port = app.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = app.accept().await.unwrap();
            let mut buf = [0u8; 4];
            sock.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping");
            sock.write_all(&vec![7u8; REPLY_LEN]).await.unwrap();
            sock.shutdown().await.unwrap();
        });

        // Puerto libre para el gateway simulado.
        let gw_addr = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        let (got_tx, mut got_rx) = mpsc::channel(1);
        tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(TunnelServiceServer::new(FakeGateway { got: got_tx }))
                .serve(gw_addr),
        );
        let endpoint = Channel::from_shared(format!("http://{gw_addr}")).unwrap();
        let channel = loop {
            match endpoint.connect().await {
                Ok(c) => break c,
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        };
        let client = TunnelServiceClient::new(channel);

        // Un puerto que no está en la lista no se toca.
        let denied = TunnelOpen {
            stream_id: "x".into(),
            port: app_port as u32,
        };
        assert!(serve(client.clone(), denied, &[]).await.is_err());

        let open = TunnelOpen {
            stream_id: "abc123".into(),
            port: app_port as u32,
        };
        tokio::time::timeout(Duration::from_secs(10), serve(client, open, &[app_port]))
            .await
            .expect("serve did not finish")
            .expect("serve failed");

        let (id, data) = got_rx.recv().await.unwrap();
        assert_eq!(id, "abc123");
        assert_eq!(data.len(), REPLY_LEN);
    }
}
