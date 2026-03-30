use anyhow::{Result, bail};
use axum::{
    Json,
    extract::{Multipart, State},
    http::StatusCode,
};
use flate2::read::GzDecoder;
use registry_client::RegistryEntry;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use tar::Archive;

use crate::{
    state::AppState,
    storage::{fs_index::IndexStore, fs_packages::PackageStore},
};

#[derive(Deserialize)]
struct TarManifest {
    package: TarPackage,
    #[serde(default)]
    dependencies: HashMap<String, String>,
}

#[derive(Deserialize)]
struct TarPackage {
    name: String,
    version: String,
}

pub async fn publish(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<RegistryEntry>), (StatusCode, String)> {
    let mut metadata = None;
    let mut package_bytes = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(internal_error)?
    {
        let name = field.name().unwrap_or_default().to_string();

        match name.as_str() {
            "metadata" => {
                let text = field.text().await.map_err(internal_error)?;
                let entry: RegistryEntry = serde_json::from_str(&text)
                    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
                metadata = Some(entry);
            }
            "package" => {
                let bytes = field.bytes().await.map_err(internal_error)?;
                package_bytes = Some(bytes.to_vec());
            }
            _ => {}
        }
    }

    let entry = metadata.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "missing metadata field in publish request".to_string(),
        )
    })?;
    let package_bytes = package_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "missing package field in publish request".to_string(),
        )
    })?;

    validate_publish_request(&entry, &package_bytes)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    let packages = PackageStore::new(state.packages_root.clone());
    let index = IndexStore::new(state.index_root.clone());
    packages
        .save_tarball(&entry.name, &entry.vers, &package_bytes)
        .map_err(internal_error)?;
    index.append_entry(&entry).map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(entry)))
}

fn validate_publish_request(entry: &RegistryEntry, package_bytes: &[u8]) -> Result<()> {
    Version::parse(&entry.vers)?;

    let manifest = read_manifest_from_tarball(package_bytes)?;

    if manifest.package.name != entry.name {
        bail!(
            "manifest package.name '{}' does not match registry metadata '{}'",
            manifest.package.name,
            entry.name
        );
    }

    if manifest.package.version != entry.vers {
        bail!(
            "manifest package.version '{}' does not match registry metadata '{}'",
            manifest.package.version,
            entry.vers
        );
    }

    let manifest_deps = manifest
        .dependencies
        .into_iter()
        .collect::<HashMap<String, String>>();
    let metadata_deps = entry
        .deps
        .iter()
        .map(|dep| (dep.name.clone(), dep.req.clone()))
        .collect::<HashMap<String, String>>();

    if manifest_deps != metadata_deps {
        bail!("manifest dependencies do not match registry metadata dependencies");
    }

    let computed_checksum = format!("sha256:{}", hex::encode(Sha256::digest(package_bytes)));
    if entry.checksum != computed_checksum {
        bail!(
            "checksum mismatch: expected '{}', got '{}'",
            entry.checksum,
            computed_checksum
        );
    }

    Ok(())
}

fn read_manifest_from_tarball(package_bytes: &[u8]) -> Result<TarManifest> {
    let decoder = GzDecoder::new(Cursor::new(package_bytes));
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        if path.file_name().and_then(|name| name.to_str()) == Some("Dora.toml") {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            let manifest: TarManifest = toml::from_str(&content)?;
            return Ok(manifest);
        }
    }

    bail!("package tarball does not contain Dora.toml")
}

fn internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
