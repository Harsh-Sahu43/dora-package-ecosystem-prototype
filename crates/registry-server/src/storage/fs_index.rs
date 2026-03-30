use anyhow::{Context, Result, bail};
use registry_client::{Registry, RegistryEntry, registry::FsRegistry};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub struct IndexStore {
    root: PathBuf,
}

impl IndexStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn package_index_path(&self, package: &str) -> PathBuf {
        self.root.join(package)
    }

    pub fn get_package_index(&self, package: &str) -> Result<String> {
        let path = self.package_index_path(package);
        fs::read_to_string(&path)
            .with_context(|| format!("failed to read registry index file {}", path.display()))
    }

    pub fn append_entry(&self, entry: &RegistryEntry) -> Result<()> {
        self.ensure_version_does_not_exist(entry)?;

        let path = self.package_index_path(&entry.name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open registry index file {}", path.display()))?;

        if path.exists() && fs::metadata(&path)?.len() > 0 {
            writeln!(file)?;
        }

        write!(file, "{}", serde_json::to_string(entry)?)?;
        Ok(())
    }

    fn ensure_version_does_not_exist(&self, entry: &RegistryEntry) -> Result<()> {
        let registry = FsRegistry::new(self.root.clone());
        let existing = registry.get_versions(&entry.name);

        if let Ok(versions) = existing {
            if versions.iter().any(|candidate| candidate.vers == entry.vers) {
                bail!(
                    "package '{}' version '{}' already exists in registry index",
                    entry.name,
                    entry.vers
                );
            }
        }

        Ok(())
    }
}
