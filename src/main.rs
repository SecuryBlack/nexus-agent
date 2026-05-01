pub mod proto {
    tonic::include_proto!("securyblack.tunnel.v1");
}

mod config;
mod registry;
mod tunnel;

use std::sync::Arc;
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
    tracing::info!("nexus-agent v{} starting…", env!("CARGO_PKG_VERSION"));

    // Cargar configuración persistente
    let cfg = match config::AgentConfig::load() {
        Ok(Some(c)) => Arc::new(c),
        Ok(None) => {
            let path = config::AgentConfig::config_path();
            tracing::error!(
                "No configuration found at {}. Please run the installer first.",
                path.display()
            );
            std::process::exit(1);
        }
        Err(e) => {
            tracing::error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    tracing::info!(
        endpoint = %cfg.endpoint,
        enabled_agents = ?cfg.enabled_agents,
        "configuration loaded"
    );

    let client = TunnelClient::new(cfg.endpoint.clone(), cfg.token.clone(), cfg.enabled_agents.clone());

    // El cliente de túnel corre en foreground porque es el core del agente.
    // Cuando el túnel cae, reconecta automáticamente.
    client.run().await;
}
