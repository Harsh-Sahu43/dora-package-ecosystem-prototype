use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub mod registry;

pub use crate::registry::{
    Dependency,
    FsRegistry,
    HttpRegistry,
    Registry,
    RegistryEntry,
};

const DEFAULT_INDEX_ROOT: &str = "registry/registry-index";

#[derive(Debug, Deserialize)]
pub struct NodeMetadata {
    pub name: String,
    pub version: String,
    pub dependencies: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct RegistryConfig {
    pub dl: String,
    pub api: Option<String>,
}

pub enum DefaultRegistry {
    Fs(FsRegistry),
    Http(HttpRegistry),
}

impl Registry for DefaultRegistry {
    fn get_versions(&self, package: &str) -> Result<Vec<RegistryEntry>> {
        match self {
            Self::Fs(registry) => registry.get_versions(package),
            Self::Http(registry) => registry.get_versions(package),
        }
    }
}

pub fn load_registry_config() -> Result<RegistryConfig> {
    let path = PathBuf::from(DEFAULT_INDEX_ROOT).join("config.json");
    let content = fs::read_to_string(path)?;
    let config = serde_json::from_str(&content)?;
    Ok(config)
}

pub fn default_registry() -> Result<DefaultRegistry> {
    let fs_registry = FsRegistry::new(PathBuf::from(DEFAULT_INDEX_ROOT));
    let config = load_registry_config()?;

    if let Some(api) = config.api {
        return Ok(DefaultRegistry::Http(HttpRegistry::new(api, config.dl)));
    }

    Ok(DefaultRegistry::Fs(fs_registry))
}

pub fn get_node_metadata(node: &str) -> Result<NodeMetadata> {
    let entry = default_registry()?.get_latest_version(node)?;
    Ok(NodeMetadata::from(entry))
}

impl From<RegistryEntry> for NodeMetadata {
    fn from(entry: RegistryEntry) -> Self {
        let dependencies = entry
            .deps
            .into_iter()
            .map(|dep| (dep.name, dep.req))
            .collect();

        Self {
            name: entry.name,
            version: entry.vers,
            dependencies,
        }
    }
}
