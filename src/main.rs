pub mod proto {
    tonic::include_proto!("securyblack.tunnel.v1");
}

mod config;
mod proxy;
mod registry;
mod tunnel;

use std::sync::Arc;
use tunnel::TunnelClient;

async fn run_agent(mut shutdown: tokio::sync::oneshot::Receiver<()>) {
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

    tokio::select! {
        _ = client.run() => {
            tracing::info!("tunnel client exited");
        }
        _ = shutdown => {
            tracing::info!("shutdown signal received, stopping");
        }
    }
}

// ── Windows Service ───────────────────────────────────────────────────────────

#[cfg(all(windows, feature = "windows-service"))]
mod service {
    use std::ffi::OsString;
    use std::time::Duration;
    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };

    const SERVICE_NAME: &str = "NexusAgent";

    define_windows_service!(ffi_service_main, service_main);

    pub fn start() -> Result<(), windows_service::Error> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
    }

    fn service_main(_arguments: Vec<OsString>) {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let shutdown_tx = std::sync::Mutex::new(Some(shutdown_tx));

        let status_handle = service_control_handler::register(
            SERVICE_NAME,
            move |control_event| match control_event {
                ServiceControl::Stop | ServiceControl::Shutdown => {
                    if let Ok(mut guard) = shutdown_tx.lock() {
                        if let Some(tx) = guard.take() {
                            let _ = tx.send(());
                        }
                    }
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            },
        )
        .expect("failed to register service control handler");

        status_handle
            .set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Running,
                controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            })
            .expect("failed to set service status Running");

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");

        rt.block_on(super::run_agent(shutdown_rx));

        let _ = status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        });
    }
}

#[cfg(all(windows, feature = "windows-service"))]
fn run_console() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    rt.block_on(async {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            let _ = shutdown_tx.send(());
        });
        run_agent(shutdown_rx).await;
    });
}

#[cfg(all(windows, feature = "windows-service"))]
fn main() {
    match service::start() {
        Ok(_) => {}
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(1063) => {
            run_console();
        }
        Err(e) => {
            eprintln!("[nexus-agent] service error: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(all(windows, feature = "windows-service")))]
#[tokio::main]
async fn main() {
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = shutdown_tx.send(());
    });
    run_agent(shutdown_rx).await;
}
