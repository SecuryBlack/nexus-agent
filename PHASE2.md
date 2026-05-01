# Fase 2: Proxy OTLP Local + Bridge de Métricas

## Contexto

En la **Fase 1** el nexus-agent se convirtió en un orquestador que detecta e informa sobre agentes locales (OxiPulse, FerroSentry, CupraFlow). Sin embargo, cada agente sigue enviando métricas de forma independiente al edge-gateway.

El objetivo de esta fase es que **OxiPulse envíe sus métricas al nexus-agent** (`localhost:4317`) y que el nexus-agent las **reenvíe al edge-gateway a través del túnel gRPC**, actuando como proxy OTLP local + bridge.

---

## Decisiones de Arquitectura

### 1. Autenticación: ¿Un token o dos?

**Decisión: Un único token por servidor.**

| Componente | Uso del token |
|---|---|
| **nexus-agent** | Autentica el túnel gRPC con el edge-gateway. |
| **OxiPulse** | Envía el mismo token en el header `Authorization: Bearer <token>` de sus métricas OTLP. |
| **Proxy local** | No valida el token; simplemente reenvía el payload completo por el túnel. |
| **edge-gateway** | Valida el token al recibir métricas por el túnel (igual que con OTLP directo). |

**Razón:** Ambos procesos corren en el mismo host y confían entre sí. Un solo token simplifica la instalación y el modelo mental para el usuario. La autenticación real sigue ocurriendo en el edge-gateway.

### 2. Modelo de Datos

| Tabla | Rol |
|---|---|
| **`agents`** | Agente principal del servidor (nexus-agent). Contiene el token único, `user_id`, `server_id`, etc. |
| **`agent_local_agents`** | Agentes locales detectados por nexus-agent (OxiPulse, FerroSentry, CupraFlow). Ya existe desde Fase 1. |
| **`agent_metrics`** | Métricas OTLP recibidas. Cuando llegan por el túnel, el edge-gateway extrae el token del payload OTLP, valida contra `agents`, y guarda con el `agent_id` correspondiente. |

> **Nota:** OxiPulse **no** tendrá su propia fila en `agents`. Sus métricas se guardan bajo el `agent_id` del nexus-agent. En el futuro se puede añadir una columna `source_agent` a `agent_metrics` para distinguir la fuente, pero por ahora no es necesario.

---

## Cambios por Proyecto

### OxiPulse (Rust)

**Objetivo:** Permitir un modo `local_agent` donde no haga falta configurar endpoint ni token obligatoriamente.

**Archivos a modificar:**
- `src/config/mod.rs` — Añadir campo `mode` (`direct` / `local_agent`).
- `scripts/install.sh` — Añadir flag `--mode local_agent`.
- `scripts/install.ps1` — Añadir parámetro `-Mode local_agent`.

**Comportamiento:**
- `mode = "direct"` (default): comportamiento actual. Pide `endpoint` y `token`.
- `mode = "local_agent"`: endpoint default a `http://localhost:4317`. El token sigue requerido (se lo inyecta el instalador del nexus-agent).

### nexus-agent (Rust)

**Objetivo:** Exponer `localhost:4317` como proxy OTLP y reenviar métricas por el túnel.

**Archivos a crear/modificar:**
- `src/proxy/mod.rs` — Servidor gRPC que implementa `ExportMetricsServiceRequest` (igual que edge-gateway).
- `src/bridge/mod.rs` — Recibe el `ExportMetricsServiceRequest`, serializa el payload, y lo pasa al túnel como `OtlpMetricsPayload`.
- `src/tunnel/mod.rs` — Añadir canal para que el bridge envíe `OtlpMetricsPayload` por el stream gRPC.
- `scripts/install.ps1` / `scripts/install.sh` — Al instalar OxiPulse, escribir su `config.toml` con:
  ```toml
  mode = "local_agent"
  endpoint = "http://localhost:4317"
  token = "<token-del-nexus-agent>"
  ```

### edge-gateway (Go)

**Objetivo:** Implementar el TODO de Fase 4: procesar métricas OTLP que llegan por el túnel.

**Archivos a modificar:**
- `internal/tunnel/server.go` — En el case `TunnelEnvelope_OtlpMetricsPayload`:
  1. Deserializar `ExportMetricsServiceRequest` del payload.
  2. Extraer el token del header OTLP embebido.
  3. Validar el token contra `agents` (reutilizar `auth.Validator`).
  4. Llamar a `parseMetrics` + `writer.Write` (reutilizar la lógica existente de `internal/grpc/server.go`).

---

## Flujo de Métricas

```
┌─────────────┐     OTLP gRPC      ┌─────────────────┐     Túnel gRPC      ┌─────────────┐
│  OxiPulse   │ ──────────────────► │  nexus-agent    │ ──────────────────► │ edge-gateway│
│  (localhost)│  Bearer <token>     │  localhost:4317 │  OtlpMetricsPayload │  (valida    │
└─────────────┘                     │  (proxy+bridge) │                     │   token)    │
                                    └─────────────────┘                     └─────────────┘
```

1. OxiPulse envía métricas OTLP a `http://localhost:4317` con su token.
2. El proxy del nexus-agent recibe el `ExportMetricsServiceRequest`.
3. El bridge serializa el request y lo envía por el túnel como `OtlpMetricsPayload`.
4. El edge-gateway deserializa, valida el token y guarda en `agent_metrics`.

---

## Progreso

- [x] **Fase 1:** Instalador interactivo + detección de agentes + reporte de estado vía túnel.
- [ ] **Fase 2:** Proxy OTLP local + Bridge de métricas
  - [ ] OxiPulse: soporte para `mode = local_agent`
  - [ ] nexus-agent: proxy OTLP en `localhost:4317`
  - [ ] nexus-agent: bridge proxy → túnel
  - [ ] edge-gateway: deserializar y persistir métricas del túnel
  - [ ] nexus-agent: inyectar config de OxiPulse automáticamente al instalar
