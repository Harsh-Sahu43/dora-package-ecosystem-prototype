use axum::{
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use tokio::fs;

use crate::state::AppState;

pub async fn get_package(
    Path((name, archive)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    if !archive.ends_with(".tar.gz") {
        return Err(StatusCode::NOT_FOUND);
    }

    let path = state
        .packages_root
        .join(&name)
        .join(&archive);

    let bytes = fs::read(path).await.map_err(|_| StatusCode::NOT_FOUND)?;
    let file_name = archive;

    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/gzip")),
            (
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&format!("attachment; filename=\"{}\"", file_name))
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            ),
        ],
        bytes,
    )
        .into_response())
}
