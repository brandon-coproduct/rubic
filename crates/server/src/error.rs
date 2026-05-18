use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("planner: {0}")]
    Planner(#[from] planner::PlannerError),
    #[error("agent: {0}")]
    Agent(#[from] agent::AgentError),
    #[error("ir: {0}")]
    Ir(#[from] core_ir::ModelError),
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("internal: {0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    detail: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, code) = match &self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::Planner(_) => (StatusCode::UNPROCESSABLE_ENTITY, "planner_error"),
            ApiError::Agent(_) => (StatusCode::BAD_GATEWAY, "agent_error"),
            ApiError::Ir(_) => (StatusCode::BAD_REQUEST, "ir_parse_error"),
            ApiError::Db(_) => (StatusCode::INTERNAL_SERVER_ERROR, "db_error"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        (
            status,
            Json(ErrorBody {
                error: code,
                detail: self.to_string(),
            }),
        )
            .into_response()
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;
