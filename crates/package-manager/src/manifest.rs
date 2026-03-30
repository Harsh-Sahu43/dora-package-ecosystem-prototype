use anyhow::{Result, bail};
use semver::Version;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

//
// RAW TOML STRUCTURE
// (matches Dora.toml exactly)
//

#[derive(Debug, Deserialize)]
pub struct TomlDoraManifest {
    pub package: TomlPackage,
    pub node: TomlNode,

    #[serde(default)]
    pub dependencies: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct TomlPackage {
    pub name: String,
    pub version: String,

    pub description: Option<String>,
    pub authors: Option<Vec<String>>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub homepage: Option<String>,
    pub keywords: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct TomlNode {
    pub language: String,
    pub entrypoint: String,
}

//
// NORMALIZED MANIFEST
//

#[derive(Debug)]
pub struct DoraManifest {
    pub name: String,
    pub version: Version,
    pub language: String,
    pub entrypoint: String,
    pub dependencies: HashMap<String, String>,
}

//
// LOADER
//

pub fn load_manifest(path: &Path) -> Result<DoraManifest> {
    let content = fs::read_to_string(path)?;

    let toml: TomlDoraManifest = toml::from_str(&content)?;

    normalize_manifest(toml)
}

//
// VALIDATION + NORMALIZATION
//

fn normalize_manifest(toml: TomlDoraManifest) -> Result<DoraManifest> {
    let version = Version::parse(&toml.package.version)?;

    if toml.package.name.trim().is_empty() {
        bail!("package.name cannot be empty");
    }

    if toml.node.entrypoint.trim().is_empty() {
        bail!("node.entrypoint cannot be empty");
    }

    Ok(DoraManifest {
        name: toml.package.name,
        version,
        language: toml.node.language,
        entrypoint: toml.node.entrypoint,
        dependencies: toml.dependencies,
    })
}
