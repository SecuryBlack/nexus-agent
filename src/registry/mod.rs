use crate::config::AgentKind;
use std::path::PathBuf;
use sysinfo::{ProcessStatus, System};

/// Estado de un agente local detectado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    /// El proceso está en ejecución en este momento.
    Running,
    /// Está instalado (tiene config o binario) pero no corre.
    Stopped,
    /// Se encontró evidencia de instalación previa pero no se puede determinar el estado.
    Installed,
    /// No se encontró rastro del agente en el sistema.
    NotInstalled,
    /// Error al consultar el estado.
    Error(String),
}

impl AgentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentStatus::Running => "running",
            AgentStatus::Stopped => "stopped",
            AgentStatus::Installed => "installed",
            AgentStatus::NotInstalled => "not_installed",
            AgentStatus::Error(_) => "error",
        }
    }
}

/// Representación de un agente local detectado.
#[derive(Debug, Clone)]
pub struct LocalAgent {
    pub kind: AgentKind,
    pub version: Option<String>,
    pub status: AgentStatus,
    pub install_path: Option<PathBuf>,
}

/// Detecta el estado de un agente local.
pub fn detect(kind: AgentKind) -> LocalAgent {
    let mut system = System::new_all();
    system.refresh_all();

    // 1. Buscar proceso en ejecución
    let process_name = kind.as_str();
    let mut found_running = false;
    for process in system.processes_by_name(process_name.as_ref()) {
        if process.status() == ProcessStatus::Run {
            found_running = true;
            break;
        }
    }

    // 2. Buscar archivo de configuración
    let config_paths = kind.config_paths();
    let found_config = config_paths.iter().any(|p| p.exists());

    // 3. Buscar binario en PATH
    let binary_name = kind.binary_name();
    let found_binary = find_in_path(&binary_name).is_some();

    // 4. Determinar estado
    let status = if found_running {
        AgentStatus::Running
    } else if found_config || found_binary {
        AgentStatus::Stopped
    } else {
        AgentStatus::NotInstalled
    };

    // 5. Intentar obtener versión
    let version = try_get_version_for_kind(&kind);

    // 6. Ruta de instalación
    let install_path = config_paths
        .iter()
        .find(|p| p.exists())
        .cloned()
        .or_else(|| find_in_path(&binary_name));

    LocalAgent {
        kind,
        version,
        status,
        install_path,
    }
}

/// Detecta todos los agentes habilitados.
pub fn detect_all(enabled: &[AgentKind]) -> Vec<LocalAgent> {
    enabled.iter().map(|k| detect(*k)).collect()
}

/// Busca un ejecutable en el PATH del sistema.
fn find_in_path(name: &str) -> Option<PathBuf> {
    if let Ok(path_env) = std::env::var("PATH") {
        let separator = if cfg!(windows) { ';' } else { ':' };
        for dir in path_env.split(separator) {
            let candidate = PathBuf::from(dir).join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Intenta obtener la versión de un agente por todos los medios disponibles.
fn try_get_version_for_kind(kind: &AgentKind) -> Option<String> {
    let binary_name = kind.binary_name();

    // Primero: ejecutar binario --version
    if let Some(binary) = find_in_path(&binary_name) {
        if let Some(v) = try_get_version_from_binary(&binary) {
            return Some(v);
        }
    }

    // Segundo: buscar binario cerca del config y ejecutarlo
    for config_path in kind.config_paths() {
        if let Some(binary) = guess_binary_from_config_dir(&config_path, &binary_name) {
            if let Some(v) = try_get_version_from_binary(&binary) {
                return Some(v);
            }
        }
    }

    // Tercero: leer config.toml
    try_get_version_from_config(kind)
}

/// Intenta ejecutar `<binary> --version` y parsear la salida.
fn try_get_version_from_binary(binary: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    extract_semver(line)
}

/// Extrae un semver-like de una cadena (implementación manual sin regex).
fn extract_semver(s: &str) -> Option<String> {
    for word in s.split_whitespace() {
        let cleaned = word.trim_start_matches('v').trim_start_matches('V');
        if cleaned.chars().all(|c| c.is_ascii_digit() || c == '.')
            && cleaned.matches('.').count() >= 1
            && !cleaned.starts_with('.')
            && !cleaned.ends_with('.')
        {
            return Some(cleaned.to_string());
        }
    }
    None
}

/// Intenta leer la versión desde el config.toml del agente.
fn try_get_version_from_config(kind: &AgentKind) -> Option<String> {
    for path in kind.config_paths() {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            for line in contents.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("version") {
                    if let Some(val) = trimmed.split('=').nth(1) {
                        let val = val.trim().trim_matches('"').trim_matches('\'');
                        return Some(val.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Dado un directorio de config, intenta adivinar dónde está el binario.
fn guess_binary_from_config_dir(config_path: &std::path::Path, binary_name: &str) -> Option<PathBuf> {
    let dir = config_path.parent()?;

    let mut candidates: Vec<PathBuf> = vec![
        dir.join(binary_name),
        dir.join("bin").join(binary_name),
    ];

    #[cfg(not(windows))]
    {
        candidates.push(PathBuf::from("/usr/local/bin").join(binary_name));
        candidates.push(PathBuf::from("/usr/bin").join(binary_name));
    }

    #[cfg(windows)]
    {
        if let Some(parent) = dir.parent() {
            candidates.push(parent.join(binary_name));
        }
    }

    for c in &candidates {
        if c.exists() {
            return Some(c.clone());
        }
    }
    None
}
