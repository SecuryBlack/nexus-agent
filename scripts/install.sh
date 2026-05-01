#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# SecuryBlack Agent (nexus-agent) — Instalador Linux/macOS
# =============================================================================
# Pregunta al usuario qué agentes locales desea instalar y configura el
# nexus-agent como servicio systemd. Si no se elige ningún agente, opera
# únicamente como túnel hacia SecuryBlack Cloud.
# =============================================================================

TOKEN=""
ENDPOINT="https://edge.securyblack.com:443"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/securyblack"
RELEASE_URL="https://github.com/securyblack/nexus-agent/releases/latest/download"

# ─── Colores ────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info()  { echo -e "${CYAN}[INFO]${NC} $*"; }
ok()    { echo -e "${GREEN}[OK]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
err()   { echo -e "${RED}[ERR]${NC} $*" >&2; }

# ─── Helpers ────────────────────────────────────────────────────────────────
detect_arch() {
    local arch
    arch="$(uname -m)"
    case "$arch" in
        x86_64)  echo "x86_64-unknown-linux-gnu" ;;
        aarch64) echo "aarch64-unknown-linux-gnu" ;;
        *)       err "Arquitectura no soportada: $arch"; exit 1 ;;
    esac
}

ask_yes_no() {
    local prompt="$1"
    local resp
    read -rp "$prompt [S/n]: " resp
    [[ -z "$resp" || "$resp" =~ ^[SsYy]$ ]]
}

# ─── Validaciones ───────────────────────────────────────────────────────────
if [[ "$EUID" -ne 0 ]]; then
    err "Este script debe ejecutarse como root (sudo)."
    exit 1
fi

info "=== SecuryBlack Agent - Instalador Linux/macOS ==="

# Token
if [[ -z "${TOKEN:-}" ]]; then
    read -rp "Introduce tu token de SecuryBlack: " TOKEN
    if [[ -z "$TOKEN" ]]; then
        err "Token requerido."
        exit 1
    fi
fi

# ─── Preguntas interactivas ─────────────────────────────────────────────────
info "Selección de agentes locales"

INSTALL_OXIPULSE=false
INSTALL_FERROSENTRY=false
INSTALL_CUPRAFLOW=false

ask_yes_no "¿Instalar OxiPulse?"    && INSTALL_OXIPULSE=true
ask_yes_no "¿Instalar FerroSentry?" && INSTALL_FERROSENTRY=true
ask_yes_no "¿Instalar CupraFlow?"   && INSTALL_CUPRAFLOW=true

ENABLED_AGENTS=()
$INSTALL_OXIPULSE    && ENABLED_AGENTS+=("oxipulse")
$INSTALL_FERROSENTRY && ENABLED_AGENTS+=("ferrosentry")
$INSTALL_CUPRAFLOW   && ENABLED_AGENTS+=("cupraflow")

if [[ ${#ENABLED_AGENTS[@]} -eq 0 ]]; then
    warn "No se seleccionó ningún agente local. El nexus-agent operará únicamente como túnel."
else
    ok "Agentes seleccionados: ${ENABLED_AGENTS[*]}"
fi

# ─── Instalar nexus-agent ───────────────────────────────────────────────────
info "Instalando SecuryBlack Agent (nexus-agent)"

ARCH="$(detect_arch)"
BINARY_NAME="nexus-agent-${ARCH}"
DOWNLOAD_URL="${RELEASE_URL}/${BINARY_NAME}"
BINARY_PATH="${INSTALL_DIR}/nexus-agent"

mkdir -p "$INSTALL_DIR"
mkdir -p "$CONFIG_DIR"

info "Descargando nexus-agent desde $DOWNLOAD_URL ..."
if command -v curl &>/dev/null; then
    curl -fsSL "$DOWNLOAD_URL" -o "$BINARY_PATH"
elif command -v wget &>/dev/null; then
    wget -q "$DOWNLOAD_URL" -O "$BINARY_PATH"
else
    err "Se requiere curl o wget."
    exit 1
fi

chmod +x "$BINARY_PATH"
ok "Binario instalado en $BINARY_PATH"

# ─── Instalar agentes seleccionados ─────────────────────────────────────────

if $INSTALL_OXIPULSE; then
    info "Instalando OxiPulse"
    if command -v oxipulse &>/dev/null || [[ -f /etc/oxipulse/config.toml ]]; then
        warn "OxiPulse parece estar ya instalado. Saltando."
    else
        # Invocar instalador oficial de OxiPulse
        OXI_URL="https://install.oxipulse.io"
        if curl -fsSL "$OXI_URL" &>/dev/null; then
            info "Invocando instalador oficial de OxiPulse ..."
            export OXIPULSE_TOKEN="$TOKEN"
            bash -c "$(curl -fsSL $OXI_URL)"
            ok "OxiPulse instalado."
        else
            warn "No se pudo contactar el instalador de OxiPulse. Instálalo manualmente."
        fi
    fi
fi

if $INSTALL_FERROSENTRY; then
    info "Instalando FerroSentry"
    FS_URL="${RELEASE_URL}/ferro-sentry-${ARCH}"
    FS_DIR="/usr/local/bin"
    FS_BIN="${FS_DIR}/ferro-sentry"
    FS_DATA="/etc/ferro-sentry"

    mkdir -p "$FS_DATA"

    info "Descargando FerroSentry ..."
    if curl -fsSL "$FS_URL" -o "$FS_BIN" 2>/dev/null; then
        chmod +x "$FS_BIN"
        cat > "${FS_DATA}/config.toml" <<EOF
token = "${TOKEN}"
mode = "agent"
api_url = "https://api.securyblack.com"
log_level = "info"
EOF
        ok "FerroSentry instalado en $FS_BIN"
        warn "FerroSentry no tiene servicio systemd aún. Ejecútalo manualmente o crea un timer."
    else
        warn "No se pudo descargar FerroSentry. Instálalo manualmente."
    fi
fi

if $INSTALL_CUPRAFLOW; then
    info "Instalando CupraFlow"
    CF_URL="${RELEASE_URL}/cupraflow-${ARCH}"
    CF_BIN="/usr/local/bin/cupraflow"
    CF_DATA="/etc/cupraflow"

    mkdir -p "$CF_DATA"

    info "Descargando CupraFlow ..."
    if curl -fsSL "$CF_URL" -o "$CF_BIN" 2>/dev/null; then
        chmod +x "$CF_BIN"
        cat > "${CF_DATA}/config.toml" <<EOF
[server]
port = 8080
bind_address = "0.0.0.0"

[logging]
level = "info"
format = "pretty"

[service]
name = "CupraFlow"
description = "Agente de gestión de red"
startup = "auto"
EOF
        ok "CupraFlow instalado en $CF_BIN"
        warn "CupraFlow no tiene servicio systemd aún. Ejecútalo manualmente."
    else
        warn "No se pudo descargar CupraFlow. Instálalo manualmente."
    fi
fi

# ─── Configurar nexus-agent ─────────────────────────────────────────────────
info "Configurando nexus-agent"

TOML_AGENTS=""
for a in "${ENABLED_AGENTS[@]}"; do
    [[ -n "$TOML_AGENTS" ]] && TOML_AGENTS+=", "
    TOML_AGENTS+="\"${a}\""
done

cat > "${CONFIG_DIR}/agent.toml" <<EOF
token = "${TOKEN}"
endpoint = "${ENDPOINT}"
enabled_agents = [${TOML_AGENTS}]
EOF

ok "Configuración escrita en ${CONFIG_DIR}/agent.toml"

# ─── Registrar servicio systemd ─────────────────────────────────────────────
info "Registrando servicio systemd"

SERVICE_NAME="securyblack-agent"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"

cat > "$SERVICE_FILE" <<EOF
[Unit]
Description=SecuryBlack Agent - Túnel y orquestador de agentes locales
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${BINARY_PATH}
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable "$SERVICE_NAME"

if systemctl start "$SERVICE_NAME"; then
    ok "Servicio ${SERVICE_NAME} iniciado correctamente."
else
    warn "El servicio no se inició automáticamente. Verifica con: journalctl -u ${SERVICE_NAME}"
fi

# ─── Resumen ────────────────────────────────────────────────────────────────
info "=== Instalación completada ==="
cat <<EOF
Ruta del binario:   ${BINARY_PATH}
Configuración:      ${CONFIG_DIR}/agent.toml
Servicio:           ${SERVICE_NAME}

Agentes habilitados: ${#ENABLED_AGENTS[@]} - ${ENABLED_AGENTS[*]:-ninguno (solo túnel)}

Comandos útiles:
  systemctl status ${SERVICE_NAME}
  journalctl -fu ${SERVICE_NAME}
  ${BINARY_PATH}   (modo consola)
EOF
