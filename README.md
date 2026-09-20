# SecuryBlack Agent (nexus-agent)

Host agent for client servers. Provides a secure, persistent outbound tunnel, orchestrates local agents (OxiPulse, FerroSentry, CupraFlow, CromoForge, TitanVault), and routes remote commands and deployments.

> **Status:** Active development. Persistent tunnel, local OTLP proxy, agent discovery registry, and config synchronization (Phases 0–2 of this roadmap) are fully implemented in `src/`. **Phase 3 (Command Routing) implemented 2026-08-24** — nexus routes `CommandRequest` payloads directly to the local command intake socket of the target agent (`FerroSentry`, `CromoForge`, ...) without interpreting domain logic, returning streaming progress back to the cloud tunnel.

---

## 🏷️ Identity & Naming

- **Product Name:** SecuryBlack Agent
- **Binary:** `nexus-agent`
- **System Service:** `securyblack-agent` (Linux systemd) / `SecuryBlackAgent` (Windows Service)

The agent serves as SecuryBlack's point of presence on client infrastructure: secure gRPC tunnel, local OTLP proxy, agent lifecycle orchestration, and command dispatch.

---

## 🏗️ General Architecture

### Core Responsibilities

1. **Persistent Tunnel:** Outbound TLS/443 gRPC stream to SecuryBlack Cloud with automated reconnect and heartbeats.
2. **Local OTLP Proxy:** Exposes `localhost:4317` (gRPC) where OxiPulse and other local telemetry sources send metrics.
3. **Telemetry Bridge:** Ingests local OTLP frames and ships them through the multiplexed tunnel to cloud ingestors.
4. **Local Agent Orchestration:** Discovers running SecuryBlack agents via sockets, runs periodic health checks, and synchronizes configuration.
5. **Auto-Configuration:** Injects local collector endpoints into co-located agents without requiring manual configuration.

### Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            SECURYBLACK CLOUD                                │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────────────────┐   │
│  │  Dashboard   │    │  Tunnel      │    │  OTLP Ingestion Engine (Go)  │   │
│  │  / Web API   │◄───┤  Server      │◄───┤  (Receives piped metrics)    │   │
│  └──────────────┘    │  (WebSocket/ │    └──────────────────────────────┘   │
│                      │   gRPC)      │                                       │
│                      └──────┬───────┘                                       │
└─────────────────────────────┼───────────────────────────────────────────────┘
                               │
                     ╔═════════╧═════════╗
                     ║   TLS TUNNEL      ║   ← Outbound 443, auto-reconnect
                     ║ (Bidirectional)   ║
                     ╚═════════╤═════════╝
                               │
┌─────────────────────────────┼───────────────────────────────────────────────┐
│     CLIENT SERVER           │                                               │
│                             │                                               │
│  ┌──────────────────────────┴─────────────────────────┐                      │
│  │  SecuryBlack Agent (nexus-agent) — Rust Service    │                      │
│  │  ┌────────────────┐  ┌──────────────────────────┐  │                      │
│  │  │ Tunnel Client  │  │ Local OTLP gRPC Server   │  │◄── Local OxiPulse    │
│  │  │ (gRPC Stream / │  │ (localhost:4317)         │  │    (local mode)      │
│  │  │  Multiplexed)  │  └──────────────────────────┘  │                      │
│  │  └────────────────┘  ┌──────────────────────────┐  │                      │
│  │  ┌────────────────┐  │ Agent Registry & Health  │  │                      │
│  │  │ Config Sync    │  │ (Discovers SB Agents)    │  │                      │
│  │  │ (Cloud ↔ Host) │  └──────────────────────────┘  │                      │
│  │  └────────────────┘                                │                      │
│  └────────────────────────────────────────────────────┘                      │
│           ▲                                                                  │
│           │ Direct OTLP gRPC (legacy direct mode)                            │
│    ┌──────┴──────┐                                                           │
│    │  OxiPulse   │  ← Direct cloud mode                                      │
│    │  (direct)   │                                                           │
│    └─────────────┘                                                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 🔌 Tunnel Protocol

**Primary Protocol:** Bidirectional streaming gRPC with native multiplexing, binary Protobuf serialization, and keepalive heartbeats.

### Protocol Definition

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

## 📁 Project Structure

```
nexus-agent/
├── Cargo.toml
├── proto/
│   └── tunnel/v1/tunnel.proto
├── src/
│   ├── main.rs                # Entry point, service wrappers, logging
│   ├── config.rs              # TOML + environment variables
│   ├── tunnel/
│   │   ├── mod.rs             # TunnelClient trait + reconnect loop
│   │   ├── grpc.rs            # Bidirectional streaming client (tonic)
│   │   ├── heartbeat.rs       # Periodic keepalive pulses
│   │   └── auth.rs            # TLS + token authentication
│   ├── proxy/
│   │   ├── mod.rs             # LocalProxy trait
│   │   └── otlp.rs            # Local OTLP gRPC server (tonic)
│   ├── bridge/
│   │   └── mod.rs             # Bridges proxy::otlp ↔ tunnel::grpc
│   ├── registry/
│   │   ├── mod.rs             # Local agent discovery
│   │   └── health.rs          # Health check probe loop
│   ├── management/
│   │   ├── mod.rs             # Command routing & config sync
│   │   └── commands.rs        # Host command dispatcher
│   └── updater/
│       └── mod.rs             # Self-updater from GitHub Releases
├── scripts/
│   ├── install.sh             # Linux installer & systemd service
│   └── install.ps1            # Windows PowerShell installer
└── .github/
    └── workflows/
        └── release.yml
```

---

## 📦 Quickstart & Installation

### Linux
```bash
curl -fsSL https://install.securyblack.dev/nexus-agent | sudo bash -s -- --token <TOKEN>
```

### Windows (PowerShell Administrator)
```powershell
irm https://install.securyblack.dev/nexus-agent/windows | iex
```

---

## License

Nexus Agent is licensed under the [Apache License, Version 2.0](LICENSE).
