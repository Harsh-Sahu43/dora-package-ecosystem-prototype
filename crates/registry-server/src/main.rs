use axum::{Router, routing::get};
use std::path::PathBuf;
use tokio::net::TcpListener;

mod state;
mod routes;
mod storage;

use state::AppState;

#[tokio::main]
async fn main() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root should be available")
        .to_path_buf();
    let state = AppState {
        index_root: workspace_root.join("registry/registry-index"),
        packages_root: workspace_root.join("registry/packages"),
    };

    let app = Router::new()
        .route("/index/:package", get(routes::index::get_index))
        .route("/packages/:name/:archive", get(routes::packages::get_package))
        .route("/publish", axum::routing::post(routes::publish::publish))
        .with_state(state);

    println!("Server running on http://localhost:8080");

    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    axum::serve(listener, app)
        .await
        .unwrap();
}
