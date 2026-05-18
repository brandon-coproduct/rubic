use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use core_ir::Model;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use replay::ReplayStore;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tokio::sync::RwLock;

pub struct AppState {
    pub model: RwLock<Model>,
    pub signing_key: SigningKey,
    pub kid: String,
    pub db: SqlitePool,
    /// Path the signing key was loaded from / persisted to. Surfaced in
    /// /healthz so operators can confirm key continuity across restarts.
    pub key_path: PathBuf,
    /// Pre-recorded agent sessions. In production this replaces live
    /// `claude -p` so visitors don't burn Anthropic tokens.
    pub replays: ReplayStore,
    /// When true, the agent endpoint will fall back to a live `claude -p`
    /// call for off-script goals. Off in prod by default.
    pub allow_live_agent: bool,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub async fn bootstrap(
        model: Model,
        db_url: &str,
        key_path: &Path,
        allow_live_agent: bool,
    ) -> anyhow::Result<SharedState> {
        let signing_key = load_or_create_signing_key(key_path)
            .with_context(|| format!("signing key at {}", key_path.display()))?;
        let kid = kid_from_verifying_key(&signing_key);

        let db = build_pool(db_url).await?;
        sqlx::migrate!("../../migrations").run(&db).await?;

        let replays = ReplayStore::from_embedded()
            .with_context(|| "load embedded replays")?;
        tracing::info!(
            replays = replays.len(),
            live_agent = allow_live_agent,
            "agent surface configured"
        );

        Ok(Arc::new(Self {
            model: RwLock::new(model),
            signing_key,
            kid,
            db,
            key_path: key_path.to_path_buf(),
            replays,
            allow_live_agent,
        }))
    }
}

async fn build_pool(url: &str) -> anyhow::Result<SqlitePool> {
    let opts: SqliteConnectOptions = url.parse()?;
    let opts = opts.create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;
    Ok(pool)
}

/// Persist signing key as the raw 32-byte seed (ed25519-dalek 2.x).
fn load_or_create_signing_key(path: &Path) -> anyhow::Result<SigningKey> {
    if path.exists() {
        let bytes = std::fs::read(path)?;
        if bytes.len() != 32 {
            anyhow::bail!(
                "signing key file {} has wrong length: {} (expected 32)",
                path.display(),
                bytes.len()
            );
        }
        let arr: [u8; 32] = bytes.try_into().unwrap();
        Ok(SigningKey::from_bytes(&arr))
    } else {
        let sk = SigningKey::generate(&mut OsRng);
        std::fs::write(path, sk.to_bytes())?;
        // POSIX: tighten perms; ignore on platforms that don't support it.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(path, perms)?;
        }
        Ok(sk)
    }
}

/// 16-hex-char prefix of the verifying key, stable across restarts. Used as
/// the `kid` field in proofs so receipts written today are matchable to a
/// rotated key later (we keep the mapping in the audit log, not the receipt).
fn kid_from_verifying_key(sk: &SigningKey) -> String {
    let vk = sk.verifying_key();
    let bytes = vk.to_bytes();
    let mut s = String::with_capacity(16);
    for b in &bytes[..8] {
        s.push_str(&format!("{:02x}", b));
    }
    s
}
