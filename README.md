# SecuryBlack Conduit

Orquestador de agentes SecuryBlack para servidores cliente. Proporciona un túnel persistente y seguro entre la infraestructura del cliente y SecuryBlack Cloud, actuando como proxy local para agentes como OxiPulse.

> **Estado:** Planificación / Diseño. Este documento recoge la arquitectura propuesta para desarrollo futuro.

---

## 🏷️ Nombre

- **Nombre del producto:** Conduit
- **Binario:** `sb-conduit`
- **Servicio systemd:** `securyblack-conduit` (Linux) / `SecuryBlackConduit` (Windows)

"Conduit" significa conducto/tubo de comunicación. Describe exactamente la función: un canal seguro y persistente por donde fluyen los datos de los agentes locales hacia SecuryBlack Cloud.

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
│  │  Conduit (sb-conduit) — Servicio Rust              │                      │
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

El `install.sh` y `install.ps1` ganan una pregunta interactiva (o flag `--mode`):

```bash
curl -fsSL https://install.oxipulse.dev | bash -s -- --mode direct --endpoint ... --token ...
curl -fsSL https://install.oxipulse.dev | bash -s -- --mode local_agent --token ...
```

Flujo interactivo:
```
[oxipulse] How do you want to send metrics?
  1) Direct to SecuryBlack Cloud (requires endpoint + token)
  2) Through the local SecuryBlack Agent (auto-configured)
Choice [1/2]: _
```

Si elige (2):
- Escribe `mode = "local_agent"` en config.toml
- Omite `endpoint`
- Pregunta solo el `token`

---

## 📁 Estructura del Proyecto Conduit (Rust)

```
sb-conduit/
├── Cargo.toml
├── proto/
│   └── tunnel.proto           ← Definición del servicio de túnel SB
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
| Auto-update | `self_update` (mismo crate que OxiPulse) |
| Windows service | `windows-service` |
| Sysinfo para inventario | `sysinfo` |

---

## 📋 Plan de Desarrollo por Fases

### Fase 0: Infraestructura compartida
- Crear el repo `sb-conduit`
- Setup de CI/CD (release cross-platform)
- Definir `proto/tunnel.proto` y publicar como artefacto compartido
- **Paralelamente:** modificar OxiPulse para soportar `mode = local_agent` (cambios mínimos descritos arriba)

### Fase 1: Túnel + Proxy OTLP (MVP)
- Implementar `tunnel::grpc` (cliente bidireccional, reconnect, heartbeat)
- Implementar `proxy::otlp` (servidor gRPC local en `localhost:4317`)
- Implementar `bridge` (recibe OTLP del proxy, empaqueta en `TunnelMessage`, envía por túnel)
- Implementar lado servidor del túnel en Go (nuevo servicio en `api-internal` o nuevo servicio)

### Fase 2: Orquestación
- `registry`: detectar si OxiPulse está corriendo localmente (chequear proceso, puerto 4317, socket Unix)
- Health checks periódicos de agentes locales
- `AgentStatus` en el túnel (informar a la nube qué agentes hay activos)
- Sincronización de config remota → local (ej: la nube dice "actualiza OxiPulse")

### Fase 3: Comandos y Gestión
- `management::commands`: ejecución remota de comandos en el servidor (tail logs, restart service, etc.)
- Configuración de OxiPulse desde Conduit (si OxiPulse no tiene endpoint, Conduit le dice "estoy aquí")
- Auto-instalación de agentes (ej: "instala OxiPulse si no está")

### Fase 4: Más agentes
- Definir contrato genérico para que cualquier agente SB se registre en Conduit
- Primer agente adicional que use el túnel (ej: log collector, security scanner)

---

## ❓ Decisiones pendientes

1. **¿Dónde vive el Tunnel Server en la nube?** ¿Servicio nuevo independiente, integrado en `api-internal` (FastAPI), o en `oxi-pulse-ingestor` (Go)?
2. **¿Qué autenticación usará Conduit para registrarse en el túnel?** ¿Un token de servidor tipo `sb_srv_xxx` distinto al token de agente de OxiPulse?
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
