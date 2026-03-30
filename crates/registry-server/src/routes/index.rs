use axum::{
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use std::path::PathBuf;
use tokio::fs;

use crate::state::AppState;

pub async fn get_index(
    Path(package): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    let path = get_index_path(&state.index_root, &package);

    let content = fs::read_to_string(path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((
        [(header::CONTENT_TYPE, HeaderValue::from_static("application/x-ndjson"))],
        content,
    )
        .into_response())
}

fn get_index_path(root: &PathBuf, name: &str) -> PathBuf {
    root.join(name)
}
