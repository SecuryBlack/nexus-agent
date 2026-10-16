# Security Policy

## Supported Versions

SecuryBlack supports the latest minor release line of Nexus Agent with security updates and bug fixes.

| Version | Supported          | Status |
| ------- | ------------------ | ------ |
| 0.2.x   | :white_check_mark: | Active support |
| < 0.2.0 | :x:                | End of life |

---

## Privileges and network access

Nexus Agent is SecuryBlack's point of presence on your server, so you should know exactly what it does before running its installer with `sudo`. This section describes the current behaviour of the code in `src/`, including its limitations.

### Installation (Linux)

The install script needs root to:

- place the binary at `/usr/local/bin/nexus-agent`;
- write `/etc/securyblack/agent.toml`, which holds your SecuryBlack server token in plain text. **The installer does not tighten its permissions yet**; run `chmod 700 /etc/securyblack && chmod 600 /etc/securyblack/agent.toml` after installing;
- register and start the systemd unit `securyblack-agent`;
- optionally run the official installers of the local agents you select (OxiPulse, FerroSentry, CromoForge, TitanVault), passing them the same token.

It downloads the release archive from GitHub Releases and verifies it against the published SHA-256 checksum before installing it. On Windows, `install.ps1` does the same under `C:\ProgramData\SecuryBlack` and registers the `NexusAgent` service.

### Runtime

- **The service runs as root** (LocalSystem on Windows). It needs this to replace its own binary, rewrite other agents' config files and restart their services.
- **No shell from the cloud.** Nexus never spawns a shell for a remote command. The only process it starts itself are `<agent> --version` (to detect local agent versions, binaries found in `PATH`) and `systemctl restart oxipulse` / `net stop|start oxipulse` after patching OxiPulse's config.
- **Config patching.** At startup, if `oxipulse` is in `enabled_agents`, Nexus rewrites `mode` and `endpoint` in `/etc/oxipulse/config.toml` (`%ProgramData%\oxipulse\config.toml` on Windows) to `local_agent` and `http://localhost:4317`, then restarts OxiPulse. No other agent config is modified.
- **Local proxy without authentication.** The gRPC server on `127.0.0.1:4317` (OTLP metrics plus FerroSentry security events) accepts any process on the host and forwards what it receives into your SecuryBlack account. It listens on loopback only.

### Remote commands

Commands arrive over the authenticated tunnel as `CommandRequest` messages.

- **For Nexus itself** (empty `target_agent`): the only implemented command is `update_now`, which runs the same signed-release update check as the daily updater. Anything else is answered with "not implemented".
- **For a local agent** (`target_agent` set): Nexus does not interpret the command. It forwards it unchanged to that agent's local command intake socket (`/run/sb-agent/<agent>-cmd.sock`, `\\.\pipe\sb-agent-cmd-<agent>` on Windows), authenticated with the machine-local shared token in `/etc/sb-agent/intake.token` (`%ProgramData%\sb-agent\intake.token`), and relays progress and the result back to the cloud. The receiving agent decides what it accepts.
- **Signed commands.** The SecuryBlack API signs each command with ed25519 over its id, a SHA-256 of your server token, target agent, command type, expiry (`expires_at`, 300 s clock skew allowed) and a SHA-256 of the payload. The public keys are built into the binary (via `sb-agent-core`). Nexus verifies commands addressed to itself; local agents verify their own in their intake. **Current limitation:** this release line runs in transition mode (`REQUIRE_SIGNED_COMMANDS = false`): an unsigned, expired or badly signed command is still accepted, with a warning in the log. The next release makes signatures mandatory.

### Outbound and local connections

| Destination | When | What is sent |
|---|---|---|
| Your `endpoint` (default `https://ingest.securyblack.com:443`), gRPC over TLS (system roots) | Persistent stream, reconnects with backoff up to 60 s | Handshake: your `token`, agent type, Nexus version, OS, architecture, host name and the list of local SecuryBlack agents (name, version, state). Then a heartbeat every 30 s, agent status every 60 s, the OTLP metrics and security events received on the local proxy, and command progress/results. If you set an `http://` endpoint, the stream is not encrypted |
| `api.github.com` / GitHub Releases | Daily (first check 5 min after start) and on `update_now` | A request for the latest `securyblack/nexus-agent` release; a newer archive is downloaded over HTTPS, its ed25519 signature is checked against the SecuryBlack keys built into the binary, and only then is the binary replaced. There is no setting to turn this off yet |
| `127.0.0.1:4317` (listens) | Always | Receives OTLP metrics and security events from local agents; nothing leaves the host except through the tunnel above |
| Local agent status sockets `/run/sb-agent/<agent>.sock` / `\\.\pipe\sb-agent-<agent>` | Every 60 s and on connect | Reads each agent's version and state. Nexus also serves its own status on `nexus-agent.sock` |
| Local agent command intake sockets | Only when a command for that agent arrives | The forwarded command plus the shared intake token |

Nexus does not contact any other host. It sends no separate telemetry; everything it reports travels over the tunnel to the endpoint you configured.

---

## Reporting a Vulnerability

We take the security of Nexus Agent and our users' infrastructure seriously. If you discover a security vulnerability, please report it through private channels.

**Please DO NOT open public GitHub issues, discussions, or pull requests for security vulnerabilities.**

Instead, please send a private email to:
**security@securyblack.com**

Please include in your report:
- A clear description of the vulnerability and potential security impact.
- Step-by-step reproduction instructions or a minimal proof-of-concept (PoC).
- Affected environment, architecture (`x86_64` or `aarch64`), and Nexus Agent version (`nexus-agent --version`).
- Any suggested mitigations or patches, if available.

### What to Expect

- **Acknowledgment**: We will acknowledge receipt of your vulnerability report within **48 hours**.
- **Assessment**: Our core security team will investigate, reproduce, and confirm the report within **5 business days**.
- **Remediation & Advisory**: Once confirmed, we will develop a patch, release an updated version, and coordinate responsible public disclosure, crediting your contribution unless you prefer to remain anonymous.
