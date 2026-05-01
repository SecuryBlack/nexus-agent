use std::time::Duration;
use tracing::{error, info, warn};

const GITHUB_OWNER: &str = "securyblack";
const GITHUB_REPO: &str = "nexus-agent";
const CHECK_INTERVAL: Duration = Duration::from_secs(86_400); // 24 hours
const STARTUP_DELAY: Duration = Duration::from_secs(300);    // 5 minutes

/// Spawn a background task that polls GitHub Releases once per day.
/// If an update is applied the process exits with code 0 so the OS
/// service manager (systemd / Windows SCM) restarts the new binary.
pub fn start_daily_check() {
    tokio::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;

        loop {
            info!("checking for updates…");
            match tokio::task::spawn_blocking(check_and_update).await {
                Ok(Ok(updated)) => {
                    if updated {
                        info!("update applied — exiting for service restart");
                        std::process::exit(0);
                    } else {
                        info!("already on latest version");
                    }
                }
                Ok(Err(e)) => warn!("update check failed: {}", e),
                Err(e) => error!("update task panicked: {}", e),
            }

            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

fn check_and_update() -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let current = env!("CARGO_PKG_VERSION");
    let target = self_update::get_target();

    let status = self_update::backends::github::Update::configure()
        .repo_owner(GITHUB_OWNER)
        .repo_name(GITHUB_REPO)
        .bin_name("nexus-agent")
        .target(&target)
        .current_version(current)
        .build()?
        .update()?;

    Ok(status.updated())
}
