use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Agente local que puede ser orquestado por nexus-agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    OxiPulse,
    FerroSentry,
    CupraFlow,
}

impl AgentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentKind::OxiPulse => "oxipulse",
            AgentKind::FerroSentry => "ferrosentry",
            AgentKind::CupraFlow => "cupraflow",
        }
    }

    /// Nombre del binario en esta plataforma.
    pub fn binary_name(&self) -> String {
        let base = self.as_str();
        if cfg!(windows) {
            format!("{}.exe", base)
        } else {
            base.to_string()
        }
    }

    /// Rutas conocidas donde el agente suele guardar su config.
    pub fn config_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if cfg!(windows) {
            if let Ok(pd) = std::env::var("ProgramData") {
                match self {
                    AgentKind::OxiPulse => {
                        paths.push(PathBuf::from(&pd).join("oxipulse").join("config.toml"));
                    }
                    AgentKind::FerroSentry => {
                        paths.push(PathBuf::from(&pd).join("ferro-sentry").join("config.toml"));
                    }
                    AgentKind::CupraFlow => {
                        paths.push(PathBuf::from(&pd).join("CupraFlow").join("config.toml"));
                    }
                }
            }
        } else {
            match self {
                AgentKind::OxiPulse => {
                    paths.push(PathBuf::from("/etc/oxipulse/config.toml"));
                }
                AgentKind::FerroSentry => {
                    paths.push(PathBuf::from("/etc/ferro-sentry/config.toml"));
                }
                AgentKind::CupraFlow => {
                    paths.push(PathBuf::from("/etc/cupraflow/config.toml"));
                }
            }
        }
        paths
    }
}

impl std::fmt::Display for AgentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for AgentKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "oxipulse" => Ok(AgentKind::OxiPulse),
            "ferrosentry" => Ok(AgentKind::FerroSentry),
            "cupraflow" => Ok(AgentKind::CupraFlow),
            _ => Err(format!("unknown agent kind: {}", s)),
        }
    }
}

/// Configuración persistente del nexus-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Token de autenticación contra SecuryBlack Cloud.
    pub token: String,

    /// Endpoint del edge-gateway.
    #[serde(default = "default_endpoint")]
    pub endpoint: String,

    /// Agentes locales que el usuario eligió habilitar.
    #[serde(default)]
    pub enabled_agents: Vec<AgentKind>,
}

fn default_endpoint() -> String {
    "https://edge.securyblack.com:443".to_string()
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            endpoint: default_endpoint(),
            enabled_agents: Vec::new(),
        }
    }
}

impl AgentConfig {
    /// Ruta del archivo de configuración según el SO.
    pub fn config_path() -> PathBuf {
        if cfg!(windows) {
            if let Ok(pd) = std::env::var("ProgramData") {
                PathBuf::from(pd).join("SecuryBlack").join("agent.toml")
            } else {
                PathBuf::from(r"C:\ProgramData\SecuryBlack\agent.toml")
            }
        } else {
            PathBuf::from("/etc/securyblack/agent.toml")
        }
    }

    /// Carga la configuración desde el archivo estándar.
    /// Si no existe, devuelve `None` para que el caller decida qué hacer.
    pub fn load() -> anyhow::Result<Option<Self>> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(None);
        }
        let contents = std::fs::read_to_string(&path)?;
        let config: AgentConfig = toml::from_str(&contents)?;
        Ok(Some(config))
    }

    /// Guarda la configuración en el archivo estándar.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }
}
