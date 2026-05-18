//! Records a live `claude -p` session for a single goal and writes the
//! result to `replays/<user>-<action>-<resource>.json` so the deployed
//! server can serve it back to visitors without spending tokens.
//!
//! Usage:
//!   cargo run --bin record-replay -p agent -- \
//!     --user alice --action read --resource payroll_summary
//!
//! The agent client honors any local `claude` CLI auth (OAuth session or
//! ANTHROPIC_API_KEY env var); this binary is meant to run on the
//! maintainer's machine, not on the deployed Fly instance.

use std::path::{Path, PathBuf};

use agent::{AgentClient, AgentResponse};
use core_ir::{Goal, Model};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Replay {
    id: String,
    goal: Goal,
    agent: AgentResponse,
    recorded_at: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let user = arg(&args, "--user").unwrap_or_else(|| "alice".to_string());
    let action = arg(&args, "--action").unwrap_or_else(|| "read".to_string());
    let resource = arg(&args, "--resource")
        .unwrap_or_else(|| "payroll_summary".to_string());
    let model_path = arg(&args, "--model")
        .unwrap_or_else(|| "examples/rbac_demo.toml".to_string());
    let out_dir = arg(&args, "--out-dir").unwrap_or_else(|| "replays".to_string());

    let model = Model::from_toml_path(&model_path)?;
    let goal = Goal {
        user: core_ir::EntityId::new(&user),
        action: action.clone(),
        resource: resource.clone(),
    };

    eprintln!("→ recording: {user} / {action} / {resource}");
    eprintln!("  model: {model_path}");

    let client = AgentClient::new();
    let agent_resp = client.propose(&model, &goal).await?;
    eprintln!(
        "  agent returned {} proposal(s) from {}",
        agent_resp.proposals.len(),
        agent_resp.model_used
    );

    let id = format!("{}-{}-{}", user, action, resource);
    let recorded_at = chrono_now_rfc3339();
    let replay = Replay {
        id: id.clone(),
        goal,
        agent: agent_resp,
        recorded_at,
    };

    let out_path = Path::new(&out_dir).join(format!("{id}.json"));
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&replay)?;
    std::fs::write(&out_path, json)?;
    eprintln!("✓ wrote {}", out_path.display());
    Ok(())
}

fn arg(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
        if let Some(rest) = a.strip_prefix(&format!("{name}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

/// We don't want to pull `chrono` in as a workspace dep just for this; format
/// the RFC3339 string by hand using `time`, which is already in the workspace.
fn chrono_now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

// Suppress the unused warning when this binary's PathBuf import (used in
// examples/docs) isn't directly referenced after argv parsing.
#[allow(dead_code)]
fn _unused() -> PathBuf {
    PathBuf::new()
}
