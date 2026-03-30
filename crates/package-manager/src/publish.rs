use crate::manifest;
use anyhow::{Context, Result, bail};
use flate2::{Compression, write::GzEncoder};
use registry_client::{Dependency, RegistryEntry, load_registry_config};
use reqwest::blocking::{Client, multipart};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tar::Builder;

pub fn publish(package_dir: &Path) -> Result<()> {
    let config = load_registry_config()?;
    let api_base = config
        .api
        .clone()
        .context("registry config must define `api` to publish packages")?;
    let manifest_path = package_dir.join("Dora.toml");
    let manifest = manifest::load_manifest(&manifest_path)
        .with_context(|| format!("failed to load manifest from {}", manifest_path.display()))?;
    let tarball = create_package_tarball(package_dir)?;
    let checksum = format!("sha256:{}", hex::encode(Sha256::digest(&tarball)));
    let entry = RegistryEntry {
        name: manifest.name.clone(),
        vers: manifest.version.to_string(),
        deps: manifest
            .dependencies
            .into_iter()
            .map(|(name, req)| Dependency { name, req })
            .collect(),
        checksum,
    };

    publish_to_registry(&api_base, entry, tarball)?;
    println!("Published {} {}", manifest.name, manifest.version);

    Ok(())
}

fn publish_to_registry(api_base: &str, entry: RegistryEntry, tarball: Vec<u8>) -> Result<()> {
    let metadata = serde_json::to_string(&entry)?;
    let file_name = format!("{}-{}.tar.gz", entry.name, entry.vers);
    let form = multipart::Form::new()
        .text("metadata", metadata)
        .part(
            "package",
            multipart::Part::bytes(tarball)
                .file_name(file_name)
                .mime_str("application/gzip")?,
        );

    let url = format!("{}/publish", api_base.trim_end_matches('/'));
    Client::new()
        .post(&url)
        .multipart(form)
        .send()
        .with_context(|| format!("failed to publish package to {}", url))?
        .error_for_status()
        .with_context(|| format!("registry server rejected publish request at {}", url))?;

    Ok(())
}

fn create_package_tarball(package_dir: &Path) -> Result<Vec<u8>> {
    if !package_dir.exists() {
        bail!("package directory not found at {}", package_dir.display());
    }

    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = Builder::new(encoder);
    append_directory_contents(&mut builder, package_dir, package_dir)?;

    let encoder = builder.into_inner()?;
    let bytes = encoder.finish()?;
    Ok(bytes)
}

fn append_directory_contents(
    builder: &mut Builder<GzEncoder<Vec<u8>>>,
    root: &Path,
    current: &Path,
) -> Result<()> {
    for entry in fs::read_dir(current)
        .with_context(|| format!("failed to read directory {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let relative_path = path
            .strip_prefix(root)
            .with_context(|| format!("failed to compute relative path for {}", path.display()))?;

        if entry.file_type()?.is_dir() {
            append_directory_contents(builder, root, &path)?;
        } else {
            builder
                .append_path_with_name(&path, relative_path)
                .with_context(|| format!("failed to add {} to package archive", path.display()))?;
        }
    }

    Ok(())
}
