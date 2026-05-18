//! Live smoke test of the agent crate against the real `claude` CLI.
//! Run from the workspace root:
//!   cargo run --example smoke -p agent

use agent::AgentClient;
use core_ir::Model;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let model = Model::from_toml_path("examples/rbac_demo.toml")?;
    let goal = model.goal.clone().expect("demo model has a goal");

    println!("Asking claude to propose roles for {} → {}:{}", goal.user, goal.action, goal.resource);
    let client = AgentClient::new();
    let resp = client.propose(&model, &goal).await?;

    println!("\n--- raw stdout ---\n{}\n--- end ---\n", resp.raw_json);
    println!("parsed {} proposals:", resp.proposals.len());
    for (i, p) in resp.proposals.iter().enumerate() {
        println!("  {}. role={}  reasoning={}", i + 1, p.role, p.reasoning);
    }
    println!("\ndigest: {}", hex(&resp.digest()));
    Ok(())
}

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for byte in b { s.push_str(&format!("{:02x}", byte)); }
    s
}
