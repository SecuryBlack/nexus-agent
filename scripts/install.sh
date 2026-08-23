#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Nexus Agent (nexus-agent) — Instalador Linux/macOS
# =============================================================================
# Instala nexus-agent como servicio systemd y, opcionalmente, los agentes
# locales. Si no se elige ningún agente, opera únicamente como túnel hacia
# SecuryBlack Cloud.
#
# Uso desatendido (el que genera el panel):
#   curl -fsSL https://install.securyblack.com/linux | sudo bash -s -- --token <TOKEN>
#
# Sin --token cae al modo interactivo y lo pide por consola.
# =============================================================================

SB_AGENT_LABEL="nexus-agent"
LIB_URL="https://raw.githubusercontent.com/securyblack/sb-agent-core/main/scripts/install-lib.sh"
LIB_TMP="$(mktemp)"
curl -fsSL "$LIB_URL" -o "$LIB_TMP" || { echo "ERROR: could not fetch install-lib.sh from sb-agent-core" >&2; exit 1; }
# shellcheck source=/dev/null
source "$LIB_TMP"
rm -f "$LIB_TMP"

TOKEN=""
ENDPOINT="https://ingest.securyblack.com:443"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/securyblack"
GITHUB_REPO="securyblack/nexus-agent"
BINARY_NAME="nexus-agent"

# Agentes instalados por defecto en modo desatendido. CupraFlow queda fuera
# a propósito: solo se instala si se pide explícitamente con --agents.
DEFAULT_AGENTS="oxipulse,ferrosentry"

AGENTS_ARG=""
ASSUME_YES=false

# ─── Helpers propios de Nexus (orquestación multi-agente, no van en la lib) ──
# El script se ejecuta casi siempre vía `curl | bash`, donde stdin es el propio
# script. Las preguntas leen de /dev/tty, que no existe en CI o en un pipe sin
# terminal: en ese caso no se puede preguntar nada.
# Comprobar `-r /dev/tty` no basta: la entrada puede existir y ser legible
# pero fallar al abrirse ("No such device or address") cuando no hay terminal
# de control. La única comprobación fiable es intentar abrirlo.
has_tty() { (exec 3</dev/tty) 2>/dev/null; }

ask_yes_no() {
    local prompt="$1"
    local resp
    read -rp "$prompt [S/n]: " resp </dev/tty
    [[ -z "$resp" || "$resp" =~ ^[SsYy]$ ]]
}

usage() {
    cat <<EOF
Instalador de Nexus Agent (SecuryBlack)

Uso:
  curl -fsSL https://install.securyblack.com/linux | sudo bash -s -- [opciones]

Opciones:
  --token <TOKEN>      Token del servidor (lo genera el panel al crearlo).
                       Si se omite, se pide de forma interactiva.
  --agents <lista>     Agentes locales separados por comas: oxipulse,ferrosentry,cupraflow
                       Usa "none" para instalar solo el túnel.
                       Por defecto en modo desatendido: ${DEFAULT_AGENTS}
  --endpoint <URL>     Endpoint de ingesta (por defecto: ${ENDPOINT})
  -y, --yes            No preguntar nada; usa los agentes por defecto.
  -h, --help           Muestra esta ayuda.
EOF
}

# ─── Argumentos ─────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --token)     TOKEN="${2:-}";     shift 2 ;;
        --token=*)   TOKEN="${1#*=}";    shift   ;;
        --agents)    AGENTS_ARG="${2:-}"; shift 2 ;;
        --agents=*)  AGENTS_ARG="${1#*=}"; shift ;;
        --endpoint)  ENDPOINT="${2:-}";  shift 2 ;;
        --endpoint=*) ENDPOINT="${1#*=}"; shift  ;;
        -y|--yes)    ASSUME_YES=true;    shift   ;;
        -h|--help)   usage; exit 0 ;;
        --)          shift ;;
        *)           sb_die "Opción desconocida: $1"; usage >&2 ;;
    esac
done

# ─── Validaciones ───────────────────────────────────────────────────────────
sb_require_root
sb_require_cmds curl tar systemctl

sb_info "=== Nexus Agent - Instalador Linux/macOS ==="

# Token
if [[ -z "${TOKEN:-}" ]]; then
    if has_tty; then
        read -rp "Introduce tu token de SecuryBlack: " TOKEN </dev/tty
    fi
    if [[ -z "$TOKEN" ]]; then
        sb_die "Token requerido. Pásalo con --token <TOKEN> o ejecuta el script en una terminal interactiva. El token se muestra en el panel al crear el servidor."
    fi
fi

# ─── Selección de agentes ───────────────────────────────────────────────────
INSTALL_OXIPULSE=false
INSTALL_FERROSENTRY=false
INSTALL_CUPRAFLOW=false

# Sin --agents: se pregunta si hay terminal, y si no (o con --yes) se usan los
# valores por defecto. Así `curl | sudo bash -s -- --token X` no se queda colgado.
if [[ -z "$AGENTS_ARG" ]]; then
    if $ASSUME_YES || ! has_tty; then
        AGENTS_ARG="$DEFAULT_AGENTS"
        sb_info "Modo desatendido: instalando ${AGENTS_ARG}"
    else
        sb_info "Selección de agentes locales"
        ask_yes_no "¿Instalar OxiPulse?"    && INSTALL_OXIPULSE=true
        ask_yes_no "¿Instalar FerroSentry?" && INSTALL_FERROSENTRY=true
        ask_yes_no "¿Instalar CupraFlow?"   && INSTALL_CUPRAFLOW=true
    fi
fi

if [[ -n "$AGENTS_ARG" ]]; then
    IFS=',' read -ra _requested <<< "$AGENTS_ARG"
    for _a in "${_requested[@]}"; do
        case "${_a// /}" in
            oxipulse)    INSTALL_OXIPULSE=true ;;
            ferrosentry) INSTALL_FERROSENTRY=true ;;
            cupraflow)   INSTALL_CUPRAFLOW=true ;;
            none|"")     ;;
            *)           sb_die "Agente desconocido: ${_a}" ;;
        esac
    done
fi

ENABLED_AGENTS=()
$INSTALL_OXIPULSE    && ENABLED_AGENTS+=("oxipulse")
$INSTALL_FERROSENTRY && ENABLED_AGENTS+=("ferrosentry")
$INSTALL_CUPRAFLOW   && ENABLED_AGENTS+=("cupraflow")

if [[ ${#ENABLED_AGENTS[@]} -eq 0 ]]; then
    sb_warn "No se seleccionó ningún agente local. El nexus-agent operará únicamente como túnel."
else
    sb_success "Agentes seleccionados: ${ENABLED_AGENTS[*]}"
fi

# ─── Instalar nexus-agent ───────────────────────────────────────────────────
sb_info "Instalando Nexus Agent (nexus-agent)"

TARGET="$(sb_detect_arch_linux)"
LATEST_VERSION="$(sb_fetch_latest_version "$GITHUB_REPO")"

mkdir -p "$INSTALL_DIR"
mkdir -p "$CONFIG_DIR"

if systemctl is-active --quiet securyblack-agent 2>/dev/null; then
    sb_info "Deteniendo servicio securyblack-agent previo..."
    systemctl stop securyblack-agent || true
fi

# El release.yml (compartido vía sb-agent-core) empaqueta el binario en un
# .tar.gz, no lo publica suelto. La versión anterior de este script descargaba
# "nexus-agent-${ARCH}" a pelo — un asset que el release nunca produce.
ASSET_NAME="${BINARY_NAME}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/download/${LATEST_VERSION}/${ASSET_NAME}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

sb_download_and_verify "$DOWNLOAD_URL" "${TMP_DIR}/${ASSET_NAME}"
sb_install_binary "${TMP_DIR}/${ASSET_NAME}" "$BINARY_NAME" "$INSTALL_DIR"
BINARY_PATH="${INSTALL_DIR}/${BINARY_NAME}"

# ─── Instalar agentes seleccionados ─────────────────────────────────────────

if $INSTALL_OXIPULSE; then
    sb_info "Instalando OxiPulse"
    if command -v oxipulse &>/dev/null || [[ -f /etc/oxipulse/config.toml ]]; then
        sb_warn "OxiPulse parece estar ya instalado. Saltando."
    else
        # Invocar instalador oficial de OxiPulse en modo local_agent
        OXI_URL="https://install.oxipulse.dev"
        if curl -fsSL "$OXI_URL" &>/dev/null; then
            sb_info "Invocando instalador oficial de OxiPulse ..."
            bash -c "$(curl -fsSL $OXI_URL)" -- --mode local_agent --token "$TOKEN"
            sb_success "OxiPulse instalado."
        else
            sb_warn "No se pudo contactar el instalador de OxiPulse. Instálalo manualmente."
        fi
    fi
fi

if $INSTALL_FERROSENTRY; then
    sb_info "Instalando FerroSentry"
    if command -v ferro-sentry &>/dev/null || [[ -f /etc/ferro-sentry/config.toml ]]; then
        sb_warn "FerroSentry parece estar ya instalado. Saltando."
    else
        FS_URL="https://install.ferrosentry.dev"
        if curl -fsSL "$FS_URL" &>/dev/null; then
            sb_info "Invocando instalador oficial de FerroSentry ..."
            bash -c "$(curl -fsSL $FS_URL)" -- --mode agent --endpoint "http://localhost:4317" --token "$TOKEN"
            sb_success "FerroSentry instalado."
        else
            sb_warn "No se pudo contactar el instalador de FerroSentry. Instálalo manualmente."
        fi
    fi
fi

if $INSTALL_CUPRAFLOW; then
    sb_warn "CupraFlow no publica todavía build para Linux (su release.yml solo compila Windows)."
    sb_warn "Sáltalo por ahora o instálalo manualmente cuando exista un target Linux."
fi

# ─── Configurar nexus-agent ─────────────────────────────────────────────────
sb_info "Configurando nexus-agent"

TOML_AGENTS=""
# `${arr[@]}` sobre un array vacío aborta con `set -u` en bash < 4.4, y el
# caso "solo túnel" (--agents none) llega aquí con la lista vacía.
for a in ${ENABLED_AGENTS[@]+"${ENABLED_AGENTS[@]}"}; do
    [[ -n "$TOML_AGENTS" ]] && TOML_AGENTS+=", "
    TOML_AGENTS+="\"${a}\""
done

cat > "${CONFIG_DIR}/agent.toml" <<EOF
token = "${TOKEN}"
endpoint = "${ENDPOINT}"
enabled_agents = [${TOML_AGENTS}]
EOF

sb_success "Configuración escrita en ${CONFIG_DIR}/agent.toml"

# ─── Registrar servicio systemd ─────────────────────────────────────────────
sb_write_systemd_unit "securyblack-agent" "Nexus Agent - Túnel y orquestador de agentes locales" "$BINARY_PATH" "" 5
sb_enable_start_service "securyblack-agent"

# ─── Resumen ────────────────────────────────────────────────────────────────
sb_info "=== Instalación completada ==="
cat <<EOF
Ruta del binario:   ${BINARY_PATH}
Configuración:      ${CONFIG_DIR}/agent.toml
Servicio:           securyblack-agent

Agentes habilitados: ${#ENABLED_AGENTS[@]} - ${ENABLED_AGENTS[*]:-ninguno (solo túnel)}

Comandos útiles:
  systemctl status securyblack-agent
  journalctl -fu securyblack-agent
  ${BINARY_PATH}   (modo consola)
EOF
