# SecuryBlack Agent (nexus-agent)

Host agent for client servers, installed from the SecuryBlack panel. It keeps a persistent outbound gRPC/TLS tunnel to SecuryBlack Cloud, runs a local OTLP proxy for the other agents, discovers the SecuryBlack agents on the host and relays remote commands to them.

> **Part of the SecuryBlack ecosystem:**
> [OxiPulse (Metrics)](https://github.com/securyblack/oxi-pulse) · [FerroSentry (Security)](https://github.com/securyblack/ferro-sentry) · [CupraFlow (Networking/HA development)](https://github.com/securyblack/cupra-flow) · [CromoForge (Docker/PostgreSQL)](https://github.com/securyblack/cromo-forge) · [TitanVault (Backups)](https://github.com/securyblack/titan-vault) · **Nexus Agent (Tunnel)** · [SecuryBlack Cloud](https://securyblack.com)

## Use with SecuryBlack

SecuryBlack is a hosted panel for server metrics, security findings and alerts. This repository contains the native agent; the hosted panel is a separate part of the product.

[Watch the 42-second product demo](https://securyblack.com/en?utm_source=github&utm_medium=referral&utm_campaign=agent_readme&utm_content=nexus-agent#how-it-works) · [Open the hosted panel](https://app.securyblack.com) · [Installation documentation](https://securyblack.com/en/docs/installation)

Follow the agent-specific installation and configuration instructions below. Use the hosted panel to obtain connection settings for your server.

> **Status:** Active development. Implemented in `src/`: the persistent tunnel with handshake, heartbeats and reconnect; the local gRPC proxy on `127.0.0.1:4317` (OTLP metrics and FerroSentry security events); local agent discovery and status reporting; auto-configuration of OxiPulse into `local_agent` mode; command routing to local agents' command intake; the `update_now` command; and the signed self-updater. Remote commands are ed25519-signed by the API; in 0.2.x unsigned commands are still accepted with a warning (see [SECURITY.md](.github/SECURITY.md)). CromoForge deployments (`DesiredState`) are defined in the protocol but not handled yet.

---

## 🏷️ Identity & Naming

- **Product Name:** SecuryBlack Agent (Nexus Agent)
- **Binary:** `nexus-agent`
- **System Service:** `securyblack-agent` (Linux systemd) / `NexusAgent` (Windows Service)
- **Config:** `/etc/securyblack/agent.toml` (Linux) / `C:\ProgramData\SecuryBlack\agent.toml` (Windows)

---

## 🏗️ General Architecture

### Core Responsibilities

1. **Persistent Tunnel:** Outbound gRPC over TLS (443) to SecuryBlack Cloud, with a heartbeat every 30 s, agent status every 60 s and exponential-backoff reconnect.
2. **Local Proxy:** Listens on `127.0.0.1:4317` for OTLP metrics (OxiPulse) and security events (FerroSentry) and ships them through the tunnel.
3. **Agent Discovery:** Reads each agent's status socket, falling back to process, config and `PATH` checks, and reports name, version and state to the cloud.
4. **Auto-Configuration:** Switches a co-located OxiPulse to `local_agent` mode pointing at the local proxy and restarts it.
5. **Command Routing:** Forwards `CommandRequest`s to the target agent's local command intake and streams progress and results back. Commands for Nexus itself: `update_now`.
6. **Self-Update:** Daily check of GitHub Releases; installs only ed25519-signed releases.

### Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            SECURYBLACK CLOUD                                │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────────────────┐   │
│  │  Dashboard   │    │  Tunnel      │    │  OTLP Ingestion Engine (Go)  │   │
│  │  / Web API   │◄───┤  Server      │◄───┤  (Receives piped metrics)    │   │
│  └──────────────┘    │  (gRPC)      │    └──────────────────────────────┘   │
│                      │              │                                       │
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

A single bidirectional gRPC stream, `TunnelService.Stream(stream TunnelEnvelope) returns (stream TunnelEnvelope)`, carries every message as a `oneof` payload: `ClientHello`/`ServerHello` handshake (the server token travels in `ClientHello`), `Heartbeat`, raw OTLP metrics, security events, `AgentStatus`, `CommandRequest`/`CommandProgress`/`CommandResponse`, and the CromoForge deployment messages. The full definition is in [`proto/tunnel/v1/tunnel.proto`](proto/tunnel/v1/tunnel.proto).

---

## 📁 Project Structure

```
nexus-agent/
├── proto/tunnel/v1/tunnel.proto   # Tunnel and local SecurityService protocol
├── build.rs                       # Compiles the proto (vendored protoc)
├── src/
│   ├── main.rs                    # Entry point, service wrapper, updater start
│   ├── config.rs                  # agent.toml and the known local agents
│   ├── tunnel/mod.rs              # gRPC tunnel client, handshake, reconnect
│   ├── proxy/mod.rs               # Local OTLP + security event proxy (:4317)
│   ├── registry/mod.rs            # Local agent discovery
│   └── management/
│       ├── mod.rs                 # OxiPulse auto-configuration
│       └── commands.rs            # Command routing and update_now
└── scripts/
    ├── install.sh                 # Linux installer and systemd service
    └── install.ps1                # Windows installer and service
```

Shared pieces (service wrapper, logging, updater, status sockets, command intake and signing) come from [`sb-agent-core`](https://github.com/securyblack/sb-agent-core).

---

## 📦 Installation

Create the server in the [SecuryBlack panel](https://app.securyblack.com); it shows these commands with your token filled in.

### Linux
```bash
curl -fsSL https://install.securyblack.com/linux | sudo bash -s -- --token <TOKEN>
```

### Windows (PowerShell as Administrator)
```powershell
& ([scriptblock]::Create((irm https://install.securyblack.com/windows))) -Token "<TOKEN>"
```

Add `--agents oxipulse,ferrosentry` (`-Agents` on Windows) to choose which local agents to install, or `none` for the tunnel only.

---

## Security

What the agent sends where, the privileges it needs and how to report a vulnerability: [SECURITY.md](.github/SECURITY.md).

## License

Nexus Agent is licensed under the [Apache License, Version 2.0](LICENSE).
