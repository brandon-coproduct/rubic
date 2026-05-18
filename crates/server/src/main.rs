mod error;
mod mcp;
mod routes;
mod state;

use std::net::SocketAddr;
use std::path::PathBuf;

use core_ir::Model;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

const DEFAULT_BIND: &str = "127.0.0.1:3000";
const DEFAULT_DB: &str = "sqlite://rubic.db";
const DEFAULT_KEY: &str = "rubic.key";
const DEFAULT_MODEL: &str = "examples/rbac_demo.toml";
/// When set, the server also serves the SPA's built `dist/` from this path.
/// In the Fly image we copy `web/dist` → `/app/static`.
const DEFAULT_STATIC: &str = "";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer())
        .init();

    let bind: SocketAddr = std::env::var("RUBIC_BIND")
        .unwrap_or_else(|_| DEFAULT_BIND.to_string())
        .parse()?;
    let db_url = std::env::var("RUBIC_DB_URL").unwrap_or_else(|_| DEFAULT_DB.to_string());
    let key_path = PathBuf::from(
        std::env::var("RUBIC_KEY_PATH").unwrap_or_else(|_| DEFAULT_KEY.to_string()),
    );
    let model_path = std::env::var("RUBIC_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    // Prod default is replay-only — set RUBIC_ALLOW_LIVE_AGENT=1 locally
    // to also fall through to a live `claude -p` call when no replay matches.
    let allow_live_agent =
        matches!(std::env::var("RUBIC_ALLOW_LIVE_AGENT").as_deref(), Ok("1" | "true"));

    let model = Model::from_toml_path(&model_path)
        .map_err(|e| anyhow::anyhow!("loading model from {model_path}: {e}"))?;
    tracing::info!(model_path = %model_path, "loaded model");

    let state =
        state::AppState::bootstrap(model, &db_url, &key_path, allow_live_agent).await?;
    tracing::info!(kid = %state.kid, "rubic server ready");

    // Build the MCP HTTP service — mounted at /mcp on the same axum app
    // so users can drive rubic from their own Claude with one config line.
    //
    // rmcp ships DNS-rebinding protection that rejects any Host header
    // not in the allowlist (default: localhost only). We extend it with
    // RUBIC_PUBLIC_HOST (the deployed hostname) so requests through the
    // public URL aren't 403'd.
    let mcp_state = state.clone();
    let mut allowed_hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    if let Ok(extra) = std::env::var("RUBIC_PUBLIC_HOST") {
        for h in extra.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            allowed_hosts.push(h.to_string());
        }
    }
    let mcp_cfg = StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts);
    let mcp_service = StreamableHttpService::new(
        move || Ok(mcp::RubicMcp::new(mcp_state.clone())),
        LocalSessionManager::default().into(),
        mcp_cfg,
    );

    let api_router = routes::router(state).nest_service("/mcp", mcp_service);

    let app = match std::env::var("RUBIC_STATIC_DIR").ok().filter(|s| !s.is_empty()) {
        Some(static_dir) => {
            // Single-binary deploy: same router serves /api/* AND the SPA
            // bundle. Fallback to index.html for client-side route 404s.
            let index = std::path::PathBuf::from(&static_dir).join("index.html");
            let serve_dir = ServeDir::new(&static_dir).fallback(ServeFile::new(&index));
            tracing::info!(static_dir = %static_dir, "serving SPA from disk");
            api_router.fallback_service(serve_dir)
        }
        None => {
            // Dev: API only. Frontend runs separately on Vite (:5173) and
            // proxies `/api/*` over.
            tracing::info!("RUBIC_STATIC_DIR unset — API only (use Vite dev for SPA)");
            api_router
        }
    }
    .layer(TraceLayer::new_for_http())
    .layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    );
    // Silence the unused-const warning when DEFAULT_STATIC is empty.
    let _ = DEFAULT_STATIC;

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "listening");
    axum::serve(listener, app).await?;
    Ok(())
}
