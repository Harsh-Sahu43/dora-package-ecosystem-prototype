use std::path::PathBuf;

#[derive(Clone)]
pub struct AppState {
    pub index_root: PathBuf,
    pub packages_root: PathBuf,
}