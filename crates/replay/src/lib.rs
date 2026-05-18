//! Pre-recorded agent sessions, bundled into the binary at compile time.
//!
//! Why: in production we don't want every visitor to spend Anthropic tokens.
//! Replays capture the live `claude -p` output for canonical demo goals
//! once at record time; the server matches an incoming goal against the
//! replay set and serves the recorded `AgentResponse` (with simulated
//! latency) verbatim. The `agent_proposal_digest` is recomputed from the
//! recorded raw JSON so receipts still bind the actual LLM bytes that
//! produced the decision.
//!
//! Off-script goals (no replay match) get a 404 with the list of available
//! goals, which the frontend renders as chips.

use agent::AgentResponse;
use core_ir::Goal;
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};

/// All replays are bundled into the binary at build time so the runtime
/// image has nothing extra to copy. Path is relative to the crate root.
static REPLAY_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../replays");

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("malformed replay {file}: {error}")]
    Parse { file: String, error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
    /// Stable id, typically derived from the file stem.
    pub id: String,
    pub goal: Goal,
    pub agent: AgentResponse,
    /// RFC3339 timestamp of when the live `claude -p` call was made.
    pub recorded_at: String,
}

#[derive(Debug, Default, Clone)]
pub struct ReplayStore {
    pub replays: Vec<Replay>,
}

impl ReplayStore {
    /// Load every `.json` file from the bundled `replays/` directory.
    /// Silently skips files that fail to parse (logged separately by the
    /// caller); returning a partial store is friendlier than crashing the
    /// whole server because one replay went stale after a schema change.
    pub fn from_embedded() -> Result<Self, ReplayError> {
        let mut replays = Vec::new();
        for f in REPLAY_DIR.files() {
            let Some(name) = f.path().file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !name.ends_with(".json") {
                continue;
            }
            let contents = f.contents_utf8().ok_or_else(|| ReplayError::Parse {
                file: name.to_string(),
                error: "not utf8".into(),
            })?;
            let r: Replay = serde_json::from_str(contents).map_err(|e| ReplayError::Parse {
                file: name.to_string(),
                error: e.to_string(),
            })?;
            replays.push(r);
        }
        // Sort by id so server boot order is stable.
        replays.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Self { replays })
    }

    /// Exact match on (user, action, resource). The whole point of replays
    /// is fidelity to a known session — fuzzy matching would mislead
    /// feedback-givers into thinking the agent considered their bespoke
    /// goal.
    pub fn find(&self, goal: &Goal) -> Option<&Replay> {
        self.replays.iter().find(|r| {
            r.goal.user == goal.user
                && r.goal.action == goal.action
                && r.goal.resource == goal.resource
        })
    }

    /// List of available goals — fed to the frontend chip picker when an
    /// off-script goal arrives.
    pub fn available_goals(&self) -> Vec<&Goal> {
        self.replays.iter().map(|r| &r.goal).collect()
    }

    pub fn len(&self) -> usize {
        self.replays.len()
    }

    pub fn is_empty(&self) -> bool {
        self.replays.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_loads_without_panic() {
        // Even if replays/ is empty at test time, load must succeed.
        let s = ReplayStore::from_embedded().expect("load");
        // Don't assert non-empty here — replays may not be recorded yet
        // when this test runs in CI on a clean checkout.
        let _ = s.len();
    }

    #[test]
    fn missing_goal_returns_none() {
        let s = ReplayStore { replays: vec![] };
        let goal = Goal {
            user: "ghost".into(),
            action: "haunt".into(),
            resource: "manor".into(),
        };
        assert!(s.find(&goal).is_none());
    }
}
