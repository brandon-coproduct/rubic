//! Shells out to the `claude` CLI in print mode (`-p`) to PROPOSE role
//! assignments. Uses the user's OAuth session — no API key required, which
//! lets the demo run anywhere `claude` is on PATH.
//!
//! The agent is treated as untrusted. Whatever JSON it returns is fed back
//! through `policy::check_plan` + the egglog reachability check; unknown
//! roles, forbidden permissions, and approval-required grants are all
//! rejected before any receipt is signed.
//!
//! **Why subprocess instead of the SDK:** Rust isn't a first-class Anthropic
//! SDK language, and `claude -p` gives us OAuth-backed auth for free.
//! Trade-offs (no direct `max_tokens` / `output_config.format` plumbing) are
//! acceptable because the server-side validator is the real trust boundary.
//!
//! ## Wire shape
//!
//! ```text
//! claude -p "<user prompt>"
//!   --append-system-prompt "<trust frame + model description>"
//!   --model sonnet
//!   --output-format text
//! ```
//!
//! stdout is captured verbatim and parsed as JSON matching
//! `{"proposals": [{"role": String, "reasoning": String}, ...]}`.

use core_ir::{Goal, Model, Permission};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub const DEFAULT_MODEL: &str = "sonnet";
pub const DEFAULT_CLAUDE_BIN: &str = "claude";
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("failed to spawn `{0}`: {1}")]
    Spawn(String, std::io::Error),
    #[error("claude exited non-zero (code {code:?}); stderr: {stderr}")]
    NonZero { code: Option<i32>, stderr: String },
    #[error("claude timed out after {0}s")]
    Timeout(u64),
    #[error("claude returned no stdout")]
    Empty,
    #[error("agent returned malformed JSON: {error}\nraw: {raw}")]
    BadJson { error: String, raw: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

// ── public types ────────────────────────────────────────────────────────────

/// One untrusted role suggestion. The string `role` goes straight through
/// `policy::check_plan`, which is the actual trust boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProposal {
    pub role: String,
    pub reasoning: String,
}

/// Full agent response, structured for receipt binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentResponse {
    /// Verbatim stdout (after trimming) — used for `agent_proposal_digest`.
    pub raw_json: String,
    pub proposals: Vec<AgentProposal>,
    pub model_used: String,
}

impl AgentResponse {
    /// BLAKE3 over the raw stdout. Bound into the receipt so the signed
    /// decision witnesses *exactly* what the LLM said, even though the
    /// content itself was never trusted.
    pub fn digest(&self) -> [u8; 32] {
        *blake3::hash(self.raw_json.as_bytes()).as_bytes()
    }
}

// ── client ──────────────────────────────────────────────────────────────────

pub struct AgentClient {
    bin: String,
    model: String,
    timeout: Duration,
}

impl Default for AgentClient {
    fn default() -> Self {
        Self {
            bin: DEFAULT_CLAUDE_BIN.to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }
}

impl AgentClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the binary path (default `claude`). Test helper.
    pub fn with_bin(mut self, bin: impl Into<String>) -> Self {
        self.bin = bin.into();
        self
    }

    /// Override the model alias (default `sonnet`).
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Ask the agent to propose roles that satisfy `goal` under `model`.
    pub async fn propose(
        &self,
        model: &Model,
        goal: &Goal,
    ) -> Result<AgentResponse, AgentError> {
        let system = render_system_prompt(model);
        let user = render_user_prompt(goal);

        // We pass the user prompt on stdin so very long goals don't blow
        // past argv limits, and so it shows up cleanly in process listings.
        // The (much larger) system prompt goes via --append-system-prompt
        // because there's no stdin equivalent for it on the CLI.
        let mut cmd = Command::new(&self.bin);
        cmd.arg("-p")
            .arg("--append-system-prompt")
            .arg(&system)
            .arg("--model")
            .arg(&self.model)
            .arg("--output-format")
            .arg("text")
            .arg("--input-format")
            .arg("text")
            .arg("--allowedTools")
            .arg("") // pure LLM output; no tool calls
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| AgentError::Spawn(self.bin.clone(), e))?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(user.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        let output = match tokio::time::timeout(self.timeout, child.wait_with_output()).await
        {
            Ok(r) => r?,
            Err(_) => return Err(AgentError::Timeout(self.timeout.as_secs())),
        };

        if !output.status.success() {
            return Err(AgentError::NonZero {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            return Err(AgentError::Empty);
        }

        // The model may wrap the JSON in a markdown fence (```json ... ```).
        // Strip that defensively before parsing.
        let json_text = extract_json(&stdout);

        let parsed: AgentJsonBody =
            serde_json::from_str(&json_text).map_err(|e| AgentError::BadJson {
                error: e.to_string(),
                raw: stdout.clone(),
            })?;

        Ok(AgentResponse {
            raw_json: stdout,
            proposals: parsed.proposals,
            model_used: self.model.clone(),
        })
    }
}

// ── prompt construction ────────────────────────────────────────────────────

pub fn render_system_prompt(m: &Model) -> String {
    let mut s = String::with_capacity(1024);
    s.push_str(
        "You are an authorization assistant for an RBAC system. Your role \
         is to PROPOSE candidate role assignments that might satisfy the \
         user's goal. You do NOT decide what is allowed — a separate \
         algebraic verifier (egglog + policy invariants) runs on every \
         proposal you return and will REJECT proposals that violate policy \
         or grant excessive permissions. Do not claim any proposal is safe; \
         claim only that it is plausible.\n\n\
         OUTPUT FORMAT: Respond with raw JSON ONLY, matching this schema:\n\
         {\"proposals\": [{\"role\": \"<role-name>\", \"reasoning\": \"<one sentence>\"}]}\n\
         No prose, no markdown fences, no preamble. Just the JSON object.\n\n\
         RULES:\n\
         - Use ONLY role names that appear verbatim in the model below. \
         Inventing role names will cause the server to reject your output.\n\
         - Prefer the role with the SMALLEST permission delta that still \
         reaches the goal (least privilege).\n\
         - If multiple roles plausibly work, return them ranked best-first.\n\
         - Do not use any tools. Do not call any functions. Just emit JSON.\n\n\
         === SEMANTIC MODEL ===\n",
    );
    s.push_str("Users:\n");
    for (uid, user) in &m.users {
        let roles: Vec<&str> = user.roles.iter().map(|r| r.as_str()).collect();
        s.push_str(&format!("  - {uid} (currently: {})\n", roles.join(", ")));
    }
    s.push_str("\nRoles and their permissions:\n");
    for (rid, role) in &m.roles {
        let perms: Vec<String> = role.permissions.iter().map(Permission::render).collect();
        s.push_str(&format!("  - {rid}: [{}]\n", perms.join(", ")));
    }
    s.push_str("\nPolicy:\n");
    s.push_str(&format!(
        "  - least_privilege: {}\n",
        m.policy.least_privilege
    ));
    let forb: Vec<String> = m
        .policy
        .forbidden_permissions
        .iter()
        .map(Permission::render)
        .collect();
    s.push_str(&format!("  - forbidden: [{}]\n", forb.join(", ")));
    let req: Vec<String> = m
        .policy
        .requires_approval
        .iter()
        .map(Permission::render)
        .collect();
    s.push_str(&format!("  - requires_approval: [{}]\n", req.join(", ")));
    s.push_str(
        "\nWhen the policy lists a permission as forbidden, any role \
         granting it will be rejected. When listed as requires_approval, \
         automatic assignment is rejected. Rank with both in mind.",
    );
    s
}

pub fn render_user_prompt(g: &Goal) -> String {
    format!(
        "Goal: user `{}` wants to perform `{}` on `{}`. \
         Propose candidate roles to assign, ranked by least privilege first. \
         Respond with raw JSON only — no markdown, no prose.",
        g.user, g.action, g.resource
    )
}

/// Strip a Markdown code fence if the model wrapped its JSON in one.
fn extract_json(s: &str) -> String {
    let trimmed = s.trim();
    // Try ```json ... ``` then ``` ... ```
    if let Some(rest) = trimmed.strip_prefix("```json") {
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim().to_string();
        }
    }
    if let Some(rest) = trimmed.strip_prefix("```") {
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim().to_string();
        }
    }
    trimmed.to_string()
}

#[derive(Debug, Deserialize)]
struct AgentJsonBody {
    proposals: Vec<AgentProposal>,
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use core_ir::Model;

    fn demo_model() -> Model {
        Model::from_toml_str(include_str!("../../../examples/rbac_demo.toml")).unwrap()
    }

    fn demo_goal() -> Goal {
        demo_model().goal.unwrap()
    }

    #[test]
    fn system_prompt_includes_every_role_and_permission() {
        let m = demo_model();
        let s = render_system_prompt(&m);
        for rid in m.roles.keys() {
            assert!(s.contains(rid.as_str()), "missing role: {rid}");
        }
        assert!(s.contains("delete:payroll"));
        assert!(s.contains("PROPOSE"));
        assert!(s.contains("egglog"));
    }

    #[test]
    fn system_prompt_is_deterministic_for_a_given_model() {
        let m = demo_model();
        assert_eq!(render_system_prompt(&m), render_system_prompt(&m));
    }

    #[test]
    fn user_prompt_renders_goal() {
        let g = demo_goal();
        let s = render_user_prompt(&g);
        assert!(s.contains("alice"));
        assert!(s.contains("read"));
        assert!(s.contains("payroll_summary"));
    }

    #[test]
    fn extract_json_strips_code_fences() {
        let raw = "```json\n{\"proposals\":[]}\n```";
        assert_eq!(extract_json(raw), "{\"proposals\":[]}");
        let raw2 = "```\n{\"proposals\":[]}\n```";
        assert_eq!(extract_json(raw2), "{\"proposals\":[]}");
        let raw3 = "{\"proposals\":[]}";
        assert_eq!(extract_json(raw3), "{\"proposals\":[]}");
    }

    #[test]
    fn agent_response_digest_is_stable_and_sensitive() {
        let r1 = AgentResponse {
            raw_json: r#"{"proposals":[{"role":"finance_viewer","reasoning":"x"}]}"#
                .to_string(),
            proposals: vec![AgentProposal {
                role: "finance_viewer".into(),
                reasoning: "x".into(),
            }],
            model_used: "sonnet".into(),
        };
        let r2 = r1.clone();
        assert_eq!(r1.digest(), r2.digest());

        let mut r3 = r1.clone();
        r3.raw_json =
            r#"{"proposals":[{"role":"payroll_admin","reasoning":"x"}]}"#.to_string();
        assert_ne!(r1.digest(), r3.digest());
    }

    /// Mock-binary integration test: substitute a shell stub for `claude`
    /// that echoes a canned JSON response. Verifies argv handling, stdin
    /// passing, and the parse path end-to-end without spending tokens.
    #[tokio::test]
    async fn end_to_end_with_mock_binary() {
        // Write a tiny shell script that ignores its args/stdin and prints
        // a deterministic JSON response.
        let dir = std::env::temp_dir().join(format!(
            "rubic-agent-mock-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("claude-mock.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\ncat > /dev/null\ncat <<'EOF'\n{\"proposals\":[{\"role\":\"finance_viewer\",\"reasoning\":\"grants read:payroll_summary at minimal cost\"},{\"role\":\"payroll_admin\",\"reasoning\":\"grants the goal permission but also write/delete payroll\"}]}\nEOF\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).unwrap();
        }

        let client = AgentClient::new().with_bin(script.to_string_lossy().to_string());
        let resp = client.propose(&demo_model(), &demo_goal()).await.unwrap();

        assert_eq!(resp.proposals.len(), 2);
        assert_eq!(resp.proposals[0].role, "finance_viewer");
        assert_eq!(resp.model_used, "sonnet");
        assert_eq!(resp.digest().len(), 32);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn mock_binary_failure_surfaces_nonzero() {
        let dir = std::env::temp_dir().join(format!(
            "rubic-agent-fail-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("claude-fail.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\ncat > /dev/null\necho 'auth error' >&2\nexit 7\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).unwrap();
        }

        let client = AgentClient::new().with_bin(script.to_string_lossy().to_string());
        let err = client
            .propose(&demo_model(), &demo_goal())
            .await
            .expect_err("should fail");
        match err {
            AgentError::NonZero { code, stderr } => {
                assert_eq!(code, Some(7));
                assert!(stderr.contains("auth"));
            }
            other => panic!("expected NonZero, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn malformed_json_is_rejected_cleanly() {
        let dir = std::env::temp_dir().join(format!(
            "rubic-agent-bad-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("claude-bad.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\ncat > /dev/null\necho 'not actually json'\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).unwrap();
        }

        let client = AgentClient::new().with_bin(script.to_string_lossy().to_string());
        let err = client
            .propose(&demo_model(), &demo_goal())
            .await
            .expect_err("should fail");
        assert!(matches!(err, AgentError::BadJson { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }
}
