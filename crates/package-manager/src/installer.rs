use crate::manifest;
use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use registry_client::load_registry_config;
use semver::Version;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use tar::Archive;

#[derive(Debug)]
pub enum InstallOutcome {
    Installed,
    Cached,
}

pub fn install_node(node: &str, version: &str) -> Result<InstallOutcome> {
    let cache_path = Path::new("node-cache").join(node).join(version);
    let archive_source = resolve_archive_source(node, version)?;

    if cache_path.exists() {
        if cache_entry_matches(node, version, &cache_path)? {
            return Ok(InstallOutcome::Cached);
        }

        fs::remove_dir_all(&cache_path).with_context(|| {
            format!(
                "failed to clear incomplete cache entry at {}",
                cache_path.display()
            )
        })?;
    }

    println!("Extracting {} {}", node, version);
    extract_archive_to_cache(&archive_source, &cache_path)?;
    validate_package_manifest(node, version, &cache_path)?;

    Ok(InstallOutcome::Installed)
}

fn extract_archive_to_cache(archive_source: &ArchiveSource, cache_path: &Path) -> Result<()> {
    let cache_parent = cache_path
        .parent()
        .context("cache path should have a parent directory")?;
    let staging_path = cache_parent.join(format!(
        ".{}.{}.staging",
        cache_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("package"),
        std::process::id()
    ));

    if staging_path.exists() {
        fs::remove_dir_all(&staging_path).with_context(|| {
            format!(
                "failed to clear stale staging directory at {}",
                staging_path.display()
            )
        })?;
    }

    fs::create_dir_all(&staging_path).with_context(|| {
        format!(
            "failed to create staging directory {}",
            staging_path.display()
        )
    })?;

    let extraction_result = extract_archive(archive_source, &staging_path)
        .and_then(|_| finalize_staged_package(&staging_path, cache_path));

    if let Err(err) = cleanup_path(&staging_path) {
        if extraction_result.is_ok() {
            return Err(err);
        }
    }

    extraction_result
}

fn extract_archive(archive_source: &ArchiveSource, destination: &Path) -> Result<()> {
    let archive_reader = archive_source.open()?;
    let decoder = GzDecoder::new(archive_reader);
    let mut archive = Archive::new(decoder);

    archive.unpack(destination).with_context(|| {
        format!(
            "failed to extract package archive {} into {}",
            archive_source.display(),
            destination.display()
        )
    })?;

    Ok(())
}

fn finalize_staged_package(staging_path: &Path, cache_path: &Path) -> Result<()> {
    let package_root = detect_package_root(staging_path)?;
    fs::create_dir_all(
        cache_path
            .parent()
            .context("cache path should have a parent directory")?,
    )
    .with_context(|| format!("failed to create cache parent for {}", cache_path.display()))?;

    if package_root == staging_path {
        fs::rename(staging_path, cache_path)
            .or_else(|_| move_dir_contents(staging_path, cache_path))
    } else {
        move_dir_contents(&package_root, cache_path)
    }
}

fn cache_entry_matches(node: &str, version: &str, cache_path: &Path) -> Result<bool> {
    let manifest_path = cache_path.join("Dora.toml");
    if !manifest_path.exists() {
        return Ok(false);
    }

    match validate_package_manifest(node, version, cache_path) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

fn validate_package_manifest(node: &str, version: &str, package_path: &Path) -> Result<()> {
    let manifest = manifest::load_manifest(&package_path.join("Dora.toml")).with_context(|| {
        format!(
            "failed to load manifest from extracted package at {}",
            package_path.display()
        )
    })?;
    let expected_version = Version::parse(version).map_err(|err| {
        anyhow::anyhow!(
            "invalid registry version `{}` for {}: {}",
            version,
            node,
            err
        )
    })?;

    if manifest.name != node {
        cleanup_incomplete_install(package_path)?;
        bail!(
            "registry package {} does not match package manifest name {}",
            node,
            manifest.name
        );
    }

    if manifest.version != expected_version {
        cleanup_incomplete_install(package_path)?;
        bail!(
            "registry index version {} does not match package manifest version {} for {}",
            version,
            manifest.version,
            node
        );
    }

    Ok(())
}

fn detect_package_root(staging_path: &Path) -> Result<PathBuf> {
    if staging_path.join("Dora.toml").exists() {
        return Ok(staging_path.to_path_buf());
    }

    let mut child_dirs = Vec::new();
    for entry in fs::read_dir(staging_path).with_context(|| {
        format!(
            "failed to read staging directory {}",
            staging_path.display()
        )
    })? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            child_dirs.push(entry.path());
        }
    }

    if child_dirs.len() == 1 && child_dirs[0].join("Dora.toml").exists() {
        return Ok(child_dirs.pop().unwrap());
    }

    bail!(
        "package archive did not contain a Dora.toml manifest at {}",
        staging_path.display()
    )
}

fn move_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)
            .with_context(|| format!("failed to clear destination {}", dst.display()))?;
    }

    fs::create_dir_all(dst)
        .with_context(|| format!("failed to create directory {}", dst.display()))?;

    for entry in
        fs::read_dir(src).with_context(|| format!("failed to read directory {}", src.display()))?
    {
        let entry = entry?;
        let destination = dst.join(entry.file_name());

        fs::rename(entry.path(), &destination)
            .or_else(|_| move_path(&entry.path(), &destination))?;
    }

    Ok(())
}

fn move_path(src: &Path, dst: &Path) -> Result<()> {
    let file_type = fs::symlink_metadata(src)
        .with_context(|| format!("failed to read file metadata for {}", src.display()))?
        .file_type();

    if file_type.is_dir() {
        move_dir_contents(src, dst)?;
        cleanup_path(src)?;
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        fs::copy(src, dst)
            .with_context(|| format!("failed to copy {} to {}", src.display(), dst.display()))?;
        fs::remove_file(src)
            .with_context(|| format!("failed to remove source file {}", src.display()))?;
    }

    Ok(())
}

fn cleanup_incomplete_install(cache_path: &Path) -> Result<()> {
    if cache_path.exists() {
        cleanup_path(cache_path)?;
    }

    Ok(())
}

fn cleanup_path(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => {
            Err(err).with_context(|| format!("failed to remove directory {}", path.display()))
        }
    }
}

enum ArchiveSource {
    Local(PathBuf),
    Remote { url: String, bytes: Vec<u8> },
}

impl ArchiveSource {
    fn open(&self) -> Result<Box<dyn Read>> {
        match self {
            Self::Local(path) => {
                let file = File::open(path)
                    .with_context(|| format!("failed to open package archive {}", path.display()))?;
                Ok(Box::new(file))
            }
            Self::Remote { bytes, .. } => Ok(Box::new(Cursor::new(bytes.clone()))),
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Local(path) => path.display().to_string(),
            Self::Remote { url, .. } => url.clone(),
        }
    }
}

fn resolve_archive_source(node: &str, version: &str) -> Result<ArchiveSource> {
    let file_name = format!("{}.tar.gz", version);

    if let Ok(config) = load_registry_config() {
        if config.dl.starts_with("http://") || config.dl.starts_with("https://") {
            let url = format!("{}/{}/{}", config.dl.trim_end_matches('/'), node, file_name);
            let bytes = Client::new()
                .get(&url)
                .send()
                .with_context(|| format!("failed to download package archive from {}", url))?
                .error_for_status()
                .with_context(|| format!("registry server returned an error for {}", url))?
                .bytes()
                .with_context(|| format!("failed to read package archive response from {}", url))?;

            return Ok(ArchiveSource::Remote {
                url,
                bytes: bytes.to_vec(),
            });
        }

        let path = Path::new(&config.dl).join(node).join(&file_name);
        if path.exists() {
            return Ok(ArchiveSource::Local(path));
        }
    }

    let fallback_path = Path::new("registry/packages").join(node).join(file_name);
    if fallback_path.exists() {
        return Ok(ArchiveSource::Local(fallback_path));
    }

    bail!(
        "package archive not found for {} {} in configured download source",
        node,
        version
    )
}

#[cfg(test)]
mod tests {
    use super::{InstallOutcome, install_node};
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::env;
    use std::fs::{self, File};
    use std::path::{Path, PathBuf};
    use std::sync::{LazyLock, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tar::Builder;

    static CWD_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    static WORKSPACE_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace directory should be available")
            .to_path_buf()
    });

    #[test]
    fn installs_archive_into_node_cache() {
        let fixture = TestFixture::new("installs_archive_into_node_cache");

        fixture.write_archive(
            "dora-yolo",
            "0.1.0",
            vec![
                ("package/Dora.toml", manifest("dora-yolo", "0.1.0")),
                ("package/index.js", "console.log('hi');\n".to_string()),
            ],
        );

        fixture.run(|| install_node("dora-yolo", "0.1.0")).unwrap();

        let manifest_path = fixture.path().join("node-cache/dora-yolo/0.1.0/Dora.toml");
        let entrypoint_path = fixture.path().join("node-cache/dora-yolo/0.1.0/index.js");

        assert!(manifest_path.exists(), "expected extracted manifest");
        assert!(
            entrypoint_path.exists(),
            "expected extracted package contents"
        );
    }

    #[test]
    fn rejects_archive_when_manifest_version_mismatches() {
        let fixture = TestFixture::new("rejects_archive_when_manifest_version_mismatches");

        fixture.write_archive(
            "dora-yolo",
            "0.1.0",
            vec![("Dora.toml", manifest("dora-yolo", "0.2.0"))],
        );

        let err = fixture
            .run(|| install_node("dora-yolo", "0.1.0"))
            .expect_err("expected install to fail");

        assert!(
            err.to_string()
                .contains("does not match package manifest version")
        );
        assert!(!fixture.path().join("node-cache/dora-yolo/0.1.0").exists());
    }

    #[test]
    fn reuses_local_cache_when_package_is_already_installed() {
        let fixture = TestFixture::new("reuses_local_cache_when_package_is_already_installed");

        fixture.write_archive(
            "dora-yolo",
            "0.1.0",
            vec![
                ("Dora.toml", manifest("dora-yolo", "0.1.0")),
                ("main.py", "print('hello')\n".to_string()),
            ],
        );

        let first = fixture.run(|| install_node("dora-yolo", "0.1.0")).unwrap();
        let second = fixture.run(|| install_node("dora-yolo", "0.1.0")).unwrap();

        assert!(matches!(first, InstallOutcome::Installed));
        assert!(matches!(second, InstallOutcome::Cached));
    }

    fn manifest(name: &str, version: &str) -> String {
        format!(
            "[package]\nname = \"{}\"\nversion = \"{}\"\n\n[node]\nlanguage = \"javascript\"\nentrypoint = \"index.js\"\n",
            name, version
        )
    }

    struct TestFixture {
        root: PathBuf,
    }

    impl TestFixture {
        fn new(test_name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = env::temp_dir().join(format!("dora-package-manager-{test_name}-{unique}"));
            fs::create_dir_all(&root).unwrap();

            Self { root }
        }

        fn path(&self) -> &Path {
            &self.root
        }

        fn run<F, T>(&self, action: F) -> anyhow::Result<T>
        where
            F: FnOnce() -> anyhow::Result<T>,
        {
            let _guard = CWD_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            env::set_current_dir(&self.root).unwrap();
            let result = action();
            env::set_current_dir(&*WORKSPACE_DIR).unwrap();
            result
        }

        fn write_archive(&self, name: &str, version: &str, files: Vec<(&str, String)>) {
            let package_dir = self.root.join("registry/packages").join(name);
            fs::create_dir_all(&package_dir).unwrap();

            let archive_path = package_dir.join(format!("{version}.tar.gz"));
            let archive_file = File::create(archive_path).unwrap();
            let encoder = GzEncoder::new(archive_file, Compression::default());
            let mut builder = Builder::new(encoder);

            for (path, content) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, path, content.as_bytes())
                    .unwrap();
            }

            builder.finish().unwrap();
        }
    }

    impl Drop for TestFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
