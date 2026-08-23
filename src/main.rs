pub mod proto {
    tonic::include_proto!("securyblack.tunnel.v1");
}

mod config;
mod management;
mod proxy;
mod registry;
mod tunnel;

use std::sync::Arc;
use tunnel::TunnelClient;

async fn run_agent(shutdown: tokio::sync::oneshot::Receiver<()>) {
    // Cargar variables desde .env (busca en directorio del ejecutable y actual)
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        let env_path = exe_dir.join(".env");
        if env_path.exists() {
            let _ = dotenvy::from_path(env_path);
        }
    }
    // Fallback: directorio actual
    let _ = dotenvy::dotenv();

    let log_dir = config::AgentConfig::config_path()
        .parent()
        .expect("config path always has a parent")
        .to_path_buf();
    sb_agent_core::logging::init("nexus-agent", &log_dir, "info");
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

    let status_handle =
        sb_agent_core::status::StatusHandle::new("nexus-agent", env!("CARGO_PKG_VERSION"));
    sb_agent_core::status::spawn_server(
        status_handle.clone(),
        sb_agent_core::status::default_socket_path("nexus-agent"),
    );
    status_handle.set_state("running");
    status_handle.set_details(serde_json::json!({
        "enabled_agents": cfg.enabled_agents.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
    }));

    management::patch_agent_configs(&cfg.enabled_agents);

    // Startup delay de 5 min (no los 60s de los demás agentes) preservado tal
    // cual — era el drift original que motivó mover esto a sb-agent-core, y
    // cambiarlo ahora sería una decisión de producto que nadie ha pedido.
    sb_agent_core::updater::start_daily_check(
        sb_agent_core::updater::UpdaterConfig::new(
            "securyblack",
            "nexus-agent",
            "nexus-agent",
            env!("CARGO_PKG_VERSION"),
        )
        .with_startup_delay(std::time::Duration::from_secs(300)),
    );

    let client = TunnelClient::new(
        cfg.endpoint.clone(),
        cfg.token.clone(),
        cfg.enabled_agents.clone(),
    );

    tokio::select! {
        _ = client.run() => {
            tracing::info!("tunnel client exited");
        }
        _ = shutdown => {
            tracing::info!("shutdown signal received, stopping");
            status_handle.set_state("stopping");
        }
    }
}

#[cfg(windows)]
fn main() {
    sb_agent_core::cli::dispatch_common_args(
        "nexus-agent",
        "nexus-agent",
        env!("CARGO_PKG_VERSION"),
    );
    match sb_agent_core::service::windows::run_service("NexusAgent", run_agent) {
        Ok(_) => {}
        Err(e) if sb_agent_core::service::windows::is_not_started_by_scm(&e) => {
            sb_agent_core::service::run_console(run_agent);
        }
        Err(e) => {
            eprintln!("[nexus-agent] service error: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(windows))]
fn main() {
    sb_agent_core::cli::dispatch_common_args(
        "nexus-agent",
        "nexus-agent",
        env!("CARGO_PKG_VERSION"),
    );
    sb_agent_core::service::run_console(run_agent);
}
