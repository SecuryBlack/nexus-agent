#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Instalador interactivo de Nexus Agent (nexus-agent) para Windows.

.DESCRIPTION
    Pregunta al usuario qué agentes locales desea instalar (OxiPulse, FerroSentry, CupraFlow)
    y configura el nexus-agent como servicio Windows. Si no se elige ningún agente,
    el nexus-agent opera únicamente como túnel hacia SecuryBlack Cloud.

.EXAMPLE
    .\install.ps1 -Token "sb_xxx"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [string]$Token = "",

    [Parameter(Mandatory = $false)]
    [string]$Endpoint = "https://edge.securyblack.com:443",

    [Parameter(Mandatory = $false)]
    [string]$InstallDir = "$env:ProgramFiles\SecuryBlack",

    [Parameter(Mandatory = $false)]
    [string]$ReleaseUrl = "https://github.com/securyblack/nexus-agent/releases/latest/download"
)

$ErrorActionPreference = "Stop"

# ─── Helpers ────────────────────────────────────────────────────────────────
function Write-Header($text) {
    Write-Host "`n=== $text ===" -ForegroundColor Cyan
}

function Write-Success($text) {
    Write-Host "[OK] $text" -ForegroundColor Green
}

function Write-Warn($text) {
    Write-Host "[WARN] $text" -ForegroundColor Yellow
}

function Write-Error($text) {
    Write-Host "[ERR] $text" -ForegroundColor Red
}

function Test-Command($cmd) {
    return [bool](Get-Command $cmd -ErrorAction SilentlyContinue)
}

function Get-Architecture {
    # nexus-agent solo soporta x86_64 por ahora
    if ($env:PROCESSOR_ARCHITECTURE -eq "AMD64") {
        return "x86_64-pc-windows-msvc"
    }
    throw "Arquitectura no soportada: $($env:PROCESSOR_ARCHITECTURE)"
}

# ─── Validaciones ───────────────────────────────────────────────────────────
Write-Header "Nexus Agent - Instalador Windows"

if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error "Este script debe ejecutarse como Administrador."
    exit 1
}

if ([string]::IsNullOrWhiteSpace($Token)) {
    $Token = Read-Host "Introduce tu token de SecuryBlack"
    if ([string]::IsNullOrWhiteSpace($Token)) {
        Write-Error "Token requerido."
        exit 1
    }
}

# ─── Preguntas interactivas ─────────────────────────────────────────────────
Write-Header "Selección de agentes locales"

function Ask-YesNo($prompt) {
    $resp = Read-Host "$prompt [S/n]"
    return ($resp -eq "" -or $resp -match "^[SsYy]")
}

$installOxiPulse    = Ask-YesNo "¿Instalar OxiPulse?"
$installFerroSentry = Ask-YesNo "¿Instalar FerroSentry?"
$installCupraFlow   = Ask-YesNo "¿Instalar CupraFlow?"

$enabledAgents = @()
if ($installOxiPulse)    { $enabledAgents += "oxipulse" }
if ($installFerroSentry) { $enabledAgents += "ferrosentry" }
if ($installCupraFlow)   { $enabledAgents += "cupraflow" }

if ($enabledAgents.Count -eq 0) {
    Write-Warn "No se seleccionó ningún agente local. El nexus-agent operará únicamente como túnel."
} else {
    Write-Success "Agentes seleccionados: $($enabledAgents -join ', ')"
}

# ─── Instalar nexus-agent ───────────────────────────────────────────────────
Write-Header "Instalando Nexus Agent (nexus-agent)"

$arch = Get-Architecture
$binaryName = "nexus-agent-$arch.exe"
$downloadUrl = "$ReleaseUrl/$binaryName"
$binaryPath = Join-Path $InstallDir "nexus-agent.exe"
$dataDir = "$env:ProgramData\SecuryBlack"

# Crear directorios
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $dataDir | Out-Null

# Descargar binario
Write-Host "Descargando nexus-agent desde $downloadUrl ..."
try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $binaryPath -UseBasicParsing
    Write-Success "Binario descargado a $binaryPath"
} catch {
    Write-Error "No se pudo descargar el binario. Verifica la URL o tu conexión."
    throw
}

# ─── Instalar agentes seleccionados ─────────────────────────────────────────

if ($installOxiPulse) {
    Write-Header "Instalando OxiPulse"
    try {
        # OxiPulse tiene su propio instalador one-liner
        $oxiPulseUrl = "https://install.oxipulse.dev"
        Write-Host "Invocando instalador oficial de OxiPulse ..."
        $oxiScript = Invoke-RestMethod -Uri $oxiPulseUrl -UseBasicParsing
        $sb = [scriptblock]::Create($oxiScript)
        & $sb -Token $Token -Mode "local_agent"
        Write-Success "OxiPulse instalado."
    } catch {
        Write-Warn "No se pudo instalar OxiPulse automáticamente. Instálalo manualmente."
        Write-Warn $_.Exception.Message
    }
}

if ($installFerroSentry) {
    Write-Header "Instalando FerroSentry"
    try {
        $fsUrl = "$ReleaseUrl/ferro-sentry-x86_64-pc-windows-msvc.exe"
        $fsDir = "$env:ProgramFiles\FerroSentry"
        $fsBin = Join-Path $fsDir "ferro-sentry.exe"
        $fsData = "$env:ProgramData\ferro-sentry"

        New-Item -ItemType Directory -Force -Path $fsDir | Out-Null
        New-Item -ItemType Directory -Force -Path $fsData | Out-Null

        Write-Host "Descargando FerroSentry ..."
        Invoke-WebRequest -Uri $fsUrl -OutFile $fsBin -UseBasicParsing

        # Escribir config básica
        $fsConfig = @"
token = "$Token"
mode = "agent"
api_url = "https://api.securyblack.com"
log_level = "info"
"@
        $fsConfig | Set-Content -Path (Join-Path $fsData "config.toml") -Encoding UTF8

        Write-Success "FerroSentry instalado en $fsDir"
        Write-Warn "FerroSentry no tiene servicio Windows aún. Ejecútalo manualmente o crea una tarea programada."
    } catch {
        Write-Warn "No se pudo instalar FerroSentry automáticamente."
        Write-Warn $_.Exception.Message
    }
}

if ($installCupraFlow) {
    Write-Header "Instalando CupraFlow"
    try {
        # CupraFlow tiene su propio install.ps1 en el repo
        $cfUrl = "https://raw.githubusercontent.com/securyblack/cupra-flow/main/scripts/install.ps1"
        Write-Host "Invocando instalador oficial de CupraFlow ..."
        $cfScript = Invoke-RestMethod -Uri $cfUrl -UseBasicParsing
        Invoke-Expression $cfScript
        Write-Success "CupraFlow instalado."
    } catch {
        Write-Warn "No se pudo instalar CupraFlow automáticamente. Instálalo manualmente."
        Write-Warn $_.Exception.Message
    }
}

# ─── Configurar nexus-agent ─────────────────────────────────────────────────
Write-Header "Configurando nexus-agent"

$agentToml = @"
token = "$Token"
endpoint = "$Endpoint"
enabled_agents = [$($enabledAgents | ForEach-Object { '"' + $_ + '"' } | Join-String -Separator ', ')]
"@

$agentToml | Set-Content -Path (Join-Path $dataDir "agent.toml") -Encoding UTF8
Write-Success "Configuración escrita en $dataDir\agent.toml"

# ─── Registrar servicio Windows ─────────────────────────────────────────────
Write-Header "Registrando servicio Windows"

$serviceName = "NexusAgent"
$displayName = "Nexus Agent"

# Eliminar servicio previo si existe
$existing = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
if ($existing) {
    Write-Warn "El servicio $serviceName ya existe. Deteniendo y eliminando ..."
    Stop-Service -Name $serviceName -Force -ErrorAction SilentlyContinue
    sc.exe delete $serviceName | Out-Null
    Start-Sleep -Seconds 2
}

# Crear servicio
$null = New-Service `
    -Name $serviceName `
    -DisplayName $displayName `
    -Description "Nexus Agent - Túnel y orquestador de agentes locales" `
    -BinaryPathName "`"$binaryPath`"" `
    -StartupType Automatic

Write-Success "Servicio $serviceName registrado."

# Iniciar servicio
Write-Host "Iniciando servicio ..."
Start-Service -Name $serviceName
Start-Sleep -Seconds 2
$svc = Get-Service -Name $serviceName
if ($svc.Status -eq "Running") {
    Write-Success "Servicio $serviceName iniciado correctamente."
} else {
    Write-Warn "El servicio no se inició automáticamente. Estado: $($svc.Status)"
}

# ─── Resumen ────────────────────────────────────────────────────────────────
Write-Header "Instalación completada"
Write-Host @"
Ruta del binario:   $binaryPath
Configuración:      $dataDir\agent.toml
Servicio:           $serviceName

Agentes habilitados: $(if ($enabledAgents.Count -eq 0) { "Ninguno (solo túnel)" } else { $enabledAgents -join ', ' })

Comandos útiles:
  Get-Service $serviceName
  Restart-Service $serviceName
  & '$binaryPath'   (modo consola, si el servicio no está corriendo)
"@
