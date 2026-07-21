# SecuryBlack Agent (nexus-agent)

Agente SecuryBlack para servidores cliente. Proporciona un túnel persistente y seguro, orquesta agentes locales (OxiPulse, FerroSentry) y gestiona despliegues CI/CD desde repos de GitHub.

> **Estado:** Planificación / Diseño. Este documento recoge la arquitectura propuesta para desarrollo futuro.

---

## 🏷️ Nombre

- **Nombre del producto:** SecuryBlack Agent
- **Binario:** `nexus-agent`
- **Servicio systemd:** `securyblack-agent` (Linux) / `SecuryBlackAgent` (Windows)

"Agente" es el punto de presencia de SecuryBlack en la infraestructura del cliente: túnel seguro, proxy OTLP local, orquestación de agentes y motor de despliegue CI/CD.

---

## 🏗️ Arquitectura General

### Responsabilidades

1. **Túnel persistente** con SecuryBlack Cloud — conexión outbound (HTTPS/443), auto-reconnect, heartbeat.
2. **Proxy OTLP local** — expone `localhost:4317` (gRPC) donde OxiPulse y futuros agentes envían métricas.
3. **Bridge** — recibe OTLP localmente y lo reenvía al ingestor de SB a través del túnel.
4. **Orquestación de agentes locales** — descubre qué agentes están corriendo, health checks, config sync.
5. **Auto-configuración** — al instalar OxiPulse en modo "a través del agente", no hace falta preguntar endpoint; Conduit inyecta la configuración.

### Diagrama de flujo

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         SECURYBLACK CLOUD                                    │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────────────────┐   │
│  │  Dashboard   │    │  Tunnel      │    │  Ingestor OTLP (Go)          │   │
│  │  / API       │◄───┤  Server      │◄───┤  (recibe metrics de Conduit) │   │
│  └──────────────┘    │  (WebSocket/ │    └──────────────────────────────┘   │
│                      │   gRPC)      │                                       │
│                      └──────┬───────┘                                       │
└─────────────────────────────┼───────────────────────────────────────────────┘
                              │
                    ╔═════════╧═════════╗
                    ║   TÚNEL TLS       ║   ← outbound 443, auto-reconnect
                    ║   (bidireccional) ║
                    ╚═════════╤═════════╝
                              │
┌─────────────────────────────┼───────────────────────────────────────────────┐
│     SERVIDOR DEL CLIENTE    │                                               │
│                             │                                               │
│  ┌──────────────────────────┴─────────────────────────┐                      │
│  │  SecuryBlack Agent (nexus-agent) — Servicio Rust      │                      │
│  │  ┌────────────────┐  ┌──────────────────────────┐  │                      │
│  │  │ Tunnel Client  │  │ Local OTLP gRPC Server   │  │◄── OxiPulse local  │
│  │  │ (WebSocket/gRPC│  │ (localhost:4317)         │  │    (modo local)    │
│  │  │  bidirectional)│  └──────────────────────────┘  │                      │
│  │  └────────────────┘  ┌──────────────────────────┐  │                      │
│  │  ┌────────────────┐  │ Agent Registry & Health  │  │                      │
│  │  │ Config Sync    │  │ (descubre agentes SB)    │  │                      │
│  │  │ (remoto ↔ local)│ └──────────────────────────┘  │                      │
│  │  └────────────────┘                                │                      │
│  └────────────────────────────────────────────────────┘                      │
│           ▲                                                                  │
│           │ OTLP gRPC directo (modo legacy)                                  │
│    ┌──────┴──────┐                                                           │
│    │  OxiPulse   │  ← modo "direct" (sin cambios, como ahora)                │
│    │  (modo      │                                                           │
│    │   directo)  │                                                           │
│    └─────────────┘                                                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 🔌 Protocolo de Túnel

**Protocolo principal:** gRPC bidireccional streaming.

Por coherencia con el stack actual (ya se usa `tonic` en Rust y gRPC en Go), gRPC bidireccional es eficiente, multiplexa mensajes y permite heartbeat nativo.

**Fallback futuro:** WebSocket sobre TLS para firewalls corporativos restrictivos, sin tocar la arquitectura.

### Definición protobuf propuesta

```protobuf
syntax = "proto3";

package securyblack.tunnel.v1;

service Tunnel {
  rpc Connect(stream TunnelMessage) returns (stream TunnelMessage);
}

message TunnelMessage {
  oneof payload {
    ClientHello       hello        = 1;
    ServerHello       server_hello = 2;
    OtelMetricsBatch  metrics      = 3;
    Heartbeat         heartbeat    = 4;
    ConfigUpdate      config       = 5;
    CommandRequest    command      = 6;
    CommandResponse   command_resp = 7;
    AgentStatus       agent_status = 8;
  }
}

message ClientHello {
  string server_id   = 1;
  string token       = 2;
  string version     = 3;
  repeated AgentInfo agents = 4;
}

message AgentInfo {
  string name     = 1;
  string version  = 2;
  string status   = 3;
}
```

---

## 🔧 Cambios en OxiPulse (mínimos)

### 1. Config (`config/mod.rs`)

Añadir campo `mode`:

```toml
# config.toml — modo directo (actual, default)
endpoint = "https://ingest.securyblack.com:4317"
token = "sb_xxx"
mode = "direct"

# config.toml — modo local agent
mode = "local_agent"
# endpoint se ignora o se setea automáticamente a localhost:4317
token = "sb_xxx"
```

```rust
pub enum Mode {
    #[serde(rename = "direct")]
    Direct,
    #[serde(rename = "local_agent")]
    LocalAgent,
}
```

- Si `mode = Direct` → `endpoint` es required (como ahora).
- Si `mode = LocalAgent` → `endpoint` es opcional, default a `http://localhost:4317`.

### 2. Telemetry (`telemetry/mod.rs`)

Sin cambios significativos. El `endpoint` que recibe `init()` vendrá seteado a `localhost:4317` cuando `mode = LocalAgent`. El token sigue enviándose en metadata OTLP; Conduit lo recibe y lo reenvía al ingestor cloud.

### 3. Scripts de instalación

Los scripts `install.sh` e `install.ps1` instalan el agente como servicio de sistema:

```bash
curl -fsSL https://install.securyblack.dev/nexus-agent | bash -s -- --token <TOKEN>
```

Flujo interactivo:
El instalador genera `/etc/securyblack/agent.toml` con el token y arranca el servicio `securyblack-agent`.

---

## 📁 Estructura del Proyecto Conduit (Rust)

```
nexus-agent/
├── Cargo.toml
├── proto/
│   └── tunnel/v1/tunnel.proto ← Definición del Conduit Protocol
├── src/
│   ├── main.rs                # Entry point, Windows service wrapper, init logging
│   ├── config.rs              # TOML + env vars, similar a OxiPulse
│   ├── tunnel/
│   │   ├── mod.rs             # Trait TunnelClient + loop de reconnect
│   │   ├── grpc.rs            # Implementación con tonic (bidirectional stream)
│   │   ├── heartbeat.rs       # Keepalive cada X segundos
│   │   └── auth.rs            # TLS + token auth
│   ├── proxy/
│   │   ├── mod.rs             # Trait LocalProxy
│   │   └── otlp.rs            # gRPC server OTLP local (tonic)
│   ├── bridge/
│   │   └── mod.rs             # Conecta proxy::otlp ↔ tunnel::grpc
│   ├── registry/
│   │   ├── mod.rs             # Descubrimiento de agentes locales
│   │   └── health.rs          # Health checks de agentes SB conocidos
│   ├── management/
│   │   ├── mod.rs             # Comandos remotos, config sync
│   │   └── commands.rs        # Ejecución de comandos en el host
│   └── updater/
│       └── mod.rs             # Auto-update desde GitHub Releases (self_update)
├── scripts/
│   ├── install.sh             # Linux/macOS: instala binario + systemd service
│   └── install.ps1            # Windows: instala binario + Windows Service
└── .github/
    └── workflows/
        └── release.yml        # Cross-compile + release
```

### Stack tecnológico propuesto

| Función | Crate |
|---------|-------|
| Async runtime | `tokio` (full) |
| Logging | `tracing` + `tracing-subscriber` + `tracing-appender` |
| gRPC / OTLP proxy local | `tonic` |
| Tunnel gRPC | `tonic` (cliente bidirectional streaming) |
| TLS | `rustls` + `tokio-rustls` |
| Serialización config | `serde` + `toml` |
| Serialización protobuf | `prost` (via tonic-build) |
| Docker client (deploys) | `bollard` |
| Auto-update | `self_update` (mismo crate que OxiPulse) |
| Windows service | `windows-service` |
| Sysinfo para inventario | `sysinfo` |

---

## 📋 Plan de Desarrollo por Fases

### Fase 0: Infraestructura compartida
- Setup de CI/CD (release cross-platform)
- Definir `proto/tunnel/v1/tunnel.proto` (Conduit Protocol) y publicar como artefacto compartido
- **Paralelamente:** modificar OxiPulse para soportar `mode = local_agent` (cambios mínimos descritos arriba)

### Fase 1: Túnel + Proxy OTLP (MVP)
- Implementar `tunnel::grpc` (cliente bidireccional, reconnect, heartbeat)
- Implementar `proxy::otlp` (servidor gRPC local en `localhost:4317`)
- Implementar `bridge` (recibe OTLP del proxy, empaqueta en `TunnelMessage`, envía por túnel)
- Implementar lado servidor del túnel en Go (dentro de `securyblack-edge-gateway`)

### Fase 2: Orquestación
- `registry`: detectar si OxiPulse está corriendo localmente (chequear proceso, puerto 4317, socket Unix)
- Health checks periódicos de agentes locales
- `AgentStatus` en el túnel (informar a la nube qué agentes hay activos)
- Sincronización de config remota → local (ej: la nube dice "actualiza OxiPulse")

### Fase 3: Comandos y Gestión
- `management::commands`: ejecución remota de comandos en el servidor (tail logs, restart service, etc.)
- Configuración de OxiPulse desde el Agent (si OxiPulse no tiene endpoint, el Agent inyecta `localhost:4317`)
- Auto-instalación de agentes (ej: "instala OxiPulse si no está")

### Fase 4: Más agentes
- Definir contrato genérico para que cualquier agente SB se registre en Conduit
- Integrar FerroSentry como agente adicional que use el túnel

---

## ❓ Decisiones pendientes

1. **✅ Resuelto:** El Tunnel Server vive en `securyblack-edge-gateway` (Go), junto al ingestor OTLP.
2. **✅ Resuelto:** Reutiliza el token de la tabla `agents` (mismo que usa OxiPulse).
3. **¿Empezamos por el MVP (Fase 1) directamente?** Es decir: túnel + proxy OTLP local + modificación mínima de OxiPulse, sin orquestación ni comandos todavía.

---

## 📚 Contexto: Arquitectura de OxiPulse (referencia)

OxiPulse es el agente de monitorización en Rust que Conduit debe integrar.

- **Repo:** `oxi-pulse/`
- **Protocolo de salida:** OTLP sobre gRPC (tonic/opentelemetry-otlp)
- **Config:** TOML en `/etc/oxipulse/config.toml` (Linux) o `C:\ProgramData\oxipulse\config.toml` (Windows)
- **Variables de entorno:** `OXIPULSE_ENDPOINT`, `OXIPULSE_TOKEN`, `OXIPULSE_INTERVAL_SECS`, etc.
- **Auto-update:** `self_update` crate desde GitHub Releases
- **Windows service:** `windows-service` crate
- **Offline buffer:** ring buffer con backoff exponencial cuando no hay conectividad TCP

---

## License

Nexus Agent is licensed under the [Apache License, Version 2.0](LICENSE).

