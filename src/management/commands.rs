//! Enrutado de `CommandRequest` que llega por el túnel. Nexus no interpreta
//! `command_type` — solo decide *a quién* va: si `target_agent` está vacío,
//! es una acción del propio nexus (arrancar/parar/instalar agentes
//! locales); si no, se reenvía tal cual al intake local de ese agente vía
//! `sb_agent_core::command_intake_client`, y las respuestas (progreso +
//! resultado final) vuelven al túnel.
//!
//! Ver `D:\infra\docs\design-command-intake.md` para el porqué de este
//! reparto de responsabilidades entre agentes.

use crate::proto::{tunnel_envelope::Payload, CommandProgress, CommandRequest, CommandResponse, TunnelEnvelope};
use sb_agent_core::command_intake::CommandEnvelope as IntakeEnvelope;
use tokio::sync::mpsc;

/// Despacha un `CommandRequest` recibido del túnel. Nunca bloquea el loop
/// principal del túnel: se llama desde una tarea `tokio::spawn` propia por
/// comando, así que un comando lento no retrasa heartbeats ni otros
/// comandos concurrentes.
pub async fn route(cmd: CommandRequest, tx: mpsc::Sender<TunnelEnvelope>) {
    if cmd.target_agent.is_empty() {
        route_to_nexus(cmd, tx).await;
    } else {
        route_to_local_agent(cmd, tx).await;
    }
}

/// Comandos dirigidos al propio nexus (sin `target_agent`). Todavía no hay
/// ningún `command_type` implementado aquí — los candidatos previstos son
/// encendido/apagado/reinicio del servidor como acción suelta (ver el
/// documento de diseño, tabla de reparto FerroSentry/nexus) y gestión de
/// agentes locales (`management::patch_agent_configs` ya cubre el caso de
/// autoconfiguración, pero no está expuesto todavía como `command_type`).
async fn route_to_nexus(cmd: CommandRequest, tx: mpsc::Sender<TunnelEnvelope>) {
    tracing::warn!(command_type = %cmd.command_type, "command targeted at nexus itself: not implemented yet");
    let response = CommandResponse {
        command_id: cmd.command_id,
        success: false,
        stdout: String::new(),
        stderr: format!("nexus-agent: command_type '{}' not implemented", cmd.command_type),
        exit_code: 1,
        duration_ms: 0,
    };
    send_envelope(&tx, Payload::CommandResp(response)).await;
}

/// Comandos dirigidos a un agente local concreto (FerroSentry, CromoForge,
/// ...): reenvío ciego al intake local de ese agente. Nexus no valida
/// `command_type` ni `payload` — es responsabilidad del agente destino
/// rechazarlos si no los reconoce (ver `sb_agent_core::command_intake`).
async fn route_to_local_agent(cmd: CommandRequest, tx: mpsc::Sender<TunnelEnvelope>) {
    let target_agent = cmd.target_agent.clone();
    let command_id = cmd.command_id.clone();

    let payload = if cmd.payload.trim().is_empty() {
        serde_json::Value::Null
    } else {
        match serde_json::from_str(&cmd.payload) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(command_id = %command_id, error = %e, "command payload is not valid JSON");
                let response = CommandResponse {
                    command_id,
                    success: false,
                    stdout: String::new(),
                    stderr: format!("invalid JSON payload: {e}"),
                    exit_code: 1,
                    duration_ms: 0,
                };
                send_envelope(&tx, Payload::CommandResp(response)).await;
                return;
            }
        }
    };

    let envelope = IntakeEnvelope {
        command_id: cmd.command_id.clone(),
        command_type: cmd.command_type.clone(),
        payload,
        timeout_secs: cmd.timeout_secs.max(0) as u32,
        // `send_command` lo sobrescribe con el token compartido de la
        // máquina antes de mandarlo — ver `sb_agent_core::intake_auth`.
        auth_token: String::new(),
    };

    // El cliente del intake es síncrono y bloqueante (mismo estilo que
    // status_client) — se corre en un hilo bloqueante para no colgar el
    // runtime async del túnel mientras el agente destino trabaja.
    let progress_tx = tx.clone();
    let command_id_for_progress = cmd.command_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        sb_agent_core::command_intake_client::send_command(&target_agent, &envelope, move |p| {
            let progress = CommandProgress {
                command_id: command_id_for_progress.clone(),
                stage: p.stage,
                message: p.message,
                percent: p.percent,
            };
            // `blocking_send`: estamos en el hilo bloqueante del intake
            // cliente, no en contexto async.
            let _ = progress_tx.blocking_send(TunnelEnvelope { payload: Some(Payload::CommandProgress(progress)) });
        })
    })
    .await;

    let response = match result {
        Ok(Ok(r)) => CommandResponse {
            command_id: r.command_id,
            success: r.success,
            stdout: r.stdout,
            stderr: r.stderr,
            exit_code: r.exit_code,
            duration_ms: r.duration_ms,
        },
        Ok(Err(e)) => CommandResponse {
            command_id: cmd.command_id,
            success: false,
            stdout: String::new(),
            stderr: e.to_string(),
            exit_code: 1,
            duration_ms: 0,
        },
        Err(e) => CommandResponse {
            command_id: cmd.command_id,
            success: false,
            stdout: String::new(),
            stderr: format!("intake client task panicked: {e}"),
            exit_code: 1,
            duration_ms: 0,
        },
    };

    send_envelope(&tx, Payload::CommandResp(response)).await;
}

async fn send_envelope(tx: &mpsc::Sender<TunnelEnvelope>, payload: Payload) {
    if tx.send(TunnelEnvelope { payload: Some(payload) }).await.is_err() {
        tracing::warn!("tunnel send channel closed while replying to a command");
    }
}
