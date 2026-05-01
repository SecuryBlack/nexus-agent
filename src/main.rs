pub mod proto {
    tonic::include_proto!("securyblack.tunnel.v1");
}

mod tunnel;

use tunnel::TunnelClient;

#[tokio::main]
async fn main() {
    // Cargar variables desde .env (busca en directorio del ejecutable y actual)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let env_path = exe_dir.join(".env");
            if env_path.exists() {
                let _ = dotenvy::from_path(env_path);
            }
        }
    }
    // Fallback: directorio actual
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt::init();
    tracing::info!("sb-agent v0.1.0 starting…");

    let endpoint = std::env::var("SB_AGENT_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());
    let token = std::env::var("SB_AGENT_TOKEN")
        .unwrap_or_else(|_| "dev_token".to_string());

    let client = TunnelClient::new(endpoint, token);

    // El cliente de túnel corre en foreground porque es el core del agente.
    // Cuando el túnel cae, reconecta automáticamente.
    client.run().await;
}
