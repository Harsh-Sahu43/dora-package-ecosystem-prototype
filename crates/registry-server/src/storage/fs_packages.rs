use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

pub struct PackageStore {
    root: PathBuf,
}

impl PackageStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn tarball_path(&self, name: &str, version: &str) -> PathBuf {
        self.root.join(name).join(format!("{}.tar.gz", version))
    }

    pub fn save_tarball(&self, name: &str, version: &str, bytes: &[u8]) -> Result<PathBuf> {
        let path = self.tarball_path(name, version);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create package directory {}", parent.display()))?;
        }

        fs::write(&path, bytes)
            .with_context(|| format!("failed to write package tarball {}", path.display()))?;

        Ok(path)
    }
}
