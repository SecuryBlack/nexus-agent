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

.EXAMPLE
    # Instalación desatendida en una línea (la que genera el panel).
    # `irm ... | iex` no admite parámetros, por eso se crea un scriptblock.
    & ([scriptblock]::Create((irm https://install.securyblack.com/windows))) -Token "sb_xxx"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [string]$Token = "",

    # Agentes locales separados por comas: oxipulse,ferrosentry,cupraflow
    # "none" instala solo el túnel. Vacío = preguntar (o usar defecto con -Yes).
    [Parameter(Mandatory = $false)]
    [string]$Agents = "",

    # No preguntar nada; usa los agentes por defecto.
    [Parameter(Mandatory = $false)]
    [switch]$Yes,

    [Parameter(Mandatory = $false)]
    [string]$Endpoint = "https://ingest.securyblack.com:443"
)

$ErrorActionPreference = "Stop"

$SbAgentLabel = "nexus-agent"
$libUrl = "https://raw.githubusercontent.com/securyblack/sb-agent-core/main/scripts/install-lib.ps1"
$libTmp = Join-Path ([System.IO.Path]::GetTempPath()) "sb-agent-core-install-lib.ps1"
Invoke-WebRequest -Uri $libUrl -OutFile $libTmp -UseBasicParsing
. $libTmp

$GithubRepo  = "securyblack/nexus-agent"
$BinaryName  = "nexus-agent.exe"
$InstallDir  = "$env:ProgramFiles\SecuryBlack"
$DataDir     = "$env:ProgramData\SecuryBlack"
$ServiceName = "NexusAgent"

# ─── Helpers propios de Nexus (orquestación multi-agente, no van en la lib) ──
function Ask-YesNo($prompt) {
    $resp = Read-Host "$prompt [S/n]"
    return ($resp -eq "" -or $resp -match "^[SsYy]")
}

Write-Host ""
Write-Host "=== Nexus Agent - Instalador Windows ===" -ForegroundColor Cyan

# Agentes instalados por defecto en modo desatendido. CupraFlow queda fuera
# a propósito: solo se instala si se pide explícitamente con -Agents.
$DefaultAgents = "oxipulse,ferrosentry"

# `iex` ejecuta sin consola interactiva asociada en algunos hosts; Read-Host
# fallaría. Si no se puede preguntar, hay que venir con -Token.
$canPrompt = -not [Console]::IsInputRedirected

if ([string]::IsNullOrWhiteSpace($Token)) {
    if ($canPrompt) {
        $Token = Read-Host "Introduce tu token de SecuryBlack"
    }
    if ([string]::IsNullOrWhiteSpace($Token)) {
        Invoke-SbFail "Token requerido. Pásalo con -Token <TOKEN>. El token se muestra en el panel al crear el servidor."
    }
}

# ─── Selección de agentes ───────────────────────────────────────────────────
$installOxiPulse    = $false
$installFerroSentry = $false
$installCupraFlow   = $false

if ([string]::IsNullOrWhiteSpace($Agents)) {
    if ($Yes -or -not $canPrompt) {
        $Agents = $DefaultAgents
        Write-Host "Modo desatendido: instalando $Agents"
    } else {
        Write-Host "`n=== Selección de agentes locales ===" -ForegroundColor Cyan
        $installOxiPulse    = Ask-YesNo "¿Instalar OxiPulse?"
        $installFerroSentry = Ask-YesNo "¿Instalar FerroSentry?"
        $installCupraFlow   = Ask-YesNo "¿Instalar CupraFlow?"
    }
}

if (-not [string]::IsNullOrWhiteSpace($Agents)) {
    foreach ($a in $Agents.Split(',')) {
        switch ($a.Trim().ToLower()) {
            "oxipulse"    { $installOxiPulse = $true }
            "ferrosentry" { $installFerroSentry = $true }
            "cupraflow"   { $installCupraFlow = $true }
            "none"        { }
            ""            { }
            default       { Invoke-SbFail "Agente desconocido: $a" }
        }
    }
}

$enabledAgents = @()
if ($installOxiPulse)    { $enabledAgents += "oxipulse" }
if ($installFerroSentry) { $enabledAgents += "ferrosentry" }
if ($installCupraFlow)   { $enabledAgents += "cupraflow" }

if ($enabledAgents.Count -eq 0) {
    Write-SbWarn "No se seleccionó ningún agente local. El nexus-agent operará únicamente como túnel."
} else {
    Write-SbSuccess "Agentes seleccionados: $($enabledAgents -join ', ')"
}

# ─── Instalar nexus-agent ───────────────────────────────────────────────────
Write-Host "`n=== Instalando Nexus Agent (nexus-agent) ===" -ForegroundColor Cyan

$target = Get-SbArchTarget
$version = Get-SbLatestVersion -GithubRepo $GithubRepo

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null

$tmpDir = [System.IO.Path]::GetTempPath() + [System.IO.Path]::GetRandomFileName()
New-Item -ItemType Directory -Path $tmpDir | Out-Null

try {
    $assetName = "nexus-agent-$target.zip"
    $zipPath = Get-SbReleaseAsset -GithubRepo $GithubRepo -Version $version -AssetName $assetName -TmpDir $tmpDir
    Install-SbBinaryFromZip -ZipPath $zipPath -BinaryName $BinaryName -InstallDir $InstallDir -ServiceName $ServiceName
    $binaryPath = Join-Path $InstallDir $BinaryName

    # ─── Instalar agentes seleccionados ─────────────────────────────────────
    if ($installOxiPulse) {
        Write-Host "`n=== Instalando OxiPulse ===" -ForegroundColor Cyan
        try {
            $oxiPulseUrl = "https://install.oxipulse.dev"
            $oxiScript = Invoke-RestMethod -Uri $oxiPulseUrl -UseBasicParsing
            $sb = [scriptblock]::Create($oxiScript)
            & $sb -Token $Token -Mode "local_agent"
            Write-SbSuccess "OxiPulse instalado."
        } catch {
            Write-SbWarn "No se pudo instalar OxiPulse automáticamente. Instálalo manualmente."
            Write-SbWarn $_.Exception.Message
        }
    }

    if ($installFerroSentry) {
        Write-Host "`n=== Instalando FerroSentry ===" -ForegroundColor Cyan
        try {
            $fsUrl = "https://install.ferrosentry.dev"
            $fsScript = Invoke-RestMethod -Uri $fsUrl -UseBasicParsing
            $sb = [scriptblock]::Create($fsScript)
            & $sb -Token $Token -Mode "agent" -Endpoint "http://localhost:4317"
            Write-SbSuccess "FerroSentry instalado."
        } catch {
            Write-SbWarn "No se pudo instalar FerroSentry automáticamente."
            Write-SbWarn $_.Exception.Message
        }
    }

    if ($installCupraFlow) {
        Write-Host "`n=== Instalando CupraFlow ===" -ForegroundColor Cyan
        try {
            $cfUrl = "https://raw.githubusercontent.com/securyblack/cupra-flow/main/scripts/install.ps1"
            $cfScript = Invoke-RestMethod -Uri $cfUrl -UseBasicParsing
            Invoke-Expression $cfScript
            Write-SbSuccess "CupraFlow instalado."
        } catch {
            Write-SbWarn "No se pudo instalar CupraFlow automáticamente. Instálalo manualmente."
            Write-SbWarn $_.Exception.Message
        }
    }

    # ─── Configurar nexus-agent ──────────────────────────────────────────────
    Write-Host "`n=== Configurando nexus-agent ===" -ForegroundColor Cyan

    $formattedAgents = ($enabledAgents | ForEach-Object { '"' + $_ + '"' }) -join ', '
    @"
version = "$version"
token = "$Token"
endpoint = "$Endpoint"
enabled_agents = [$formattedAgents]
"@ | Set-Content -Path (Join-Path $DataDir "agent.toml") -Encoding UTF8
    Write-SbSuccess "Configuración escrita en $DataDir\agent.toml"

    # ─── Registrar servicio Windows ──────────────────────────────────────────
    Register-SbWindowsService -ServiceName $ServiceName -DisplayName "Nexus Agent" `
        -BinaryPath "`"$binaryPath`"" `
        -Description "Nexus Agent - Túnel y orquestador de agentes locales"

} finally {
    Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
}

# ─── Resumen ────────────────────────────────────────────────────────────────
Write-Host "`n=== Instalación completada ===" -ForegroundColor Cyan
Write-Host @"
Ruta del binario:   $InstallDir\$BinaryName
Configuración:      $DataDir\agent.toml
Servicio:           $ServiceName

Agentes habilitados: $(if ($enabledAgents.Count -eq 0) { "Ninguno (solo túnel)" } else { $enabledAgents -join ', ' })

Comandos útiles:
  Get-Service $ServiceName
  Restart-Service $ServiceName
"@
