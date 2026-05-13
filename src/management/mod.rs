use crate::config::AgentKind;
use std::fs;
use std::path::PathBuf;

const LOCAL_ENDPOINT: &str = "http://localhost:4317";

pub fn patch_agent_configs(enabled_agents: &[AgentKind]) {
    for agent in enabled_agents {
        match agent {
            AgentKind::OxiPulse => patch_oxipulse(),
            AgentKind::FerroSentry => patch_ferrosentry(),
            AgentKind::CupraFlow => patch_cupraflow(),
        }
    }
}

fn patch_oxipulse() {
    let config_path = oxipulse_config_path();
    if !config_path.exists() {
        tracing::info!("OxiPulse config not found at {}, skipping patch", config_path.display());
        return;
    }

    let contents = match fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to read OxiPulse config: {}", e);
            return;
        }
    };

    let needs_patch = needs_oxipulse_patch(&contents);

    if !needs_patch {
        tracing::info!("OxiPulse config already in local_agent mode");
        return;
    }

    tracing::info!("Patching OxiPulse config to local_agent mode (endpoint → {})", LOCAL_ENDPOINT);

    let patched = patch_oxipulse_config(&contents);
    if let Err(e) = fs::write(&config_path, patched) {
        tracing::error!("Failed to write OxiPulse config: {}", e);
        return;
    }

    restart_service("oxipulse");
}

fn needs_oxipulse_patch(contents: &str) -> bool {
    let mut mode_ok = false;
    let mut endpoint_ok = false;

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match key {
                "mode" => mode_ok = value == "local_agent",
                "endpoint" => endpoint_ok = value == LOCAL_ENDPOINT,
                _ => {}
            }
        }
    }

    !(mode_ok && endpoint_ok)
}

fn patch_oxipulse_config(contents: &str) -> String {
    let mut lines: Vec<String> = contents.lines().map(String::from).collect();
    let mut had_mode = false;
    let mut had_endpoint = false;

    for line in lines.iter_mut() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = trimmed.split_once('=') {
            let key = key.trim();
            match key {
                "mode" => {
                    *line = "mode = \"local_agent\"".to_string();
                    had_mode = true;
                }
                "endpoint" => {
                    *line = format!("endpoint = \"{}\"", LOCAL_ENDPOINT);
                    had_endpoint = true;
                }
                _ => {}
            }
        }
    }

    if !had_mode {
        lines.push("mode = \"local_agent\"".to_string());
    }
    if !had_endpoint {
        lines.push(format!("endpoint = \"{}\"", LOCAL_ENDPOINT));
    }

    lines.join("\n") + "\n"
}

fn oxipulse_config_path() -> PathBuf {
    if cfg!(windows) {
        let pd = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_string());
        PathBuf::from(pd).join("oxipulse").join("config.toml")
    } else {
        PathBuf::from("/etc/oxipulse/config.toml")
    }
}

fn patch_ferrosentry() {}
fn patch_cupraflow() {}

fn restart_service(name: &str) {
    #[cfg(not(windows))]
    {
        let output = std::process::Command::new("systemctl")
            .args(["restart", name])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                tracing::info!("Restarted service {}", name);
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                tracing::warn!("Failed to restart {}: {}", name, stderr.trim());
            }
            Err(e) => {
                tracing::warn!("Failed to execute systemctl restart {}: {}", name, e);
            }
        }
    }

    #[cfg(windows)]
    {
        let output = std::process::Command::new("net")
            .args(["stop", name])
            .output();

        if let Ok(o) = output {
            if o.status.success() {
                tracing::info!("Stopped service {}", name);
            }
        }

        let output = std::process::Command::new("net")
            .args(["start", name])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                tracing::info!("Started service {}", name);
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                tracing::warn!("Failed to start {}: {}", name, stderr.trim());
            }
            Err(e) => {
                tracing::warn!("Failed to start service {}: {}", name, e);
            }
        }
    }
}