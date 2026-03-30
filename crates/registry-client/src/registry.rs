use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use semver::Version;


#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RegistryEntry {
    pub name: String,
    pub vers: String,
    pub deps: Vec<Dependency>,
    #[serde(default)]
    pub checksum: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Dependency {
    pub name: String,
    pub req: String,
}

pub trait Registry {
    fn get_versions(&self, package: &str) -> Result<Vec<RegistryEntry>>;


    fn get_latest_version(&self, package: &str) -> Result<RegistryEntry> {
        let versions = self.get_versions(package)?;

        let mut best: Option<(Version, RegistryEntry)> = None;

        for entry in versions {
            let parsed = Version::parse(&entry.vers)?;

            match &best {
                None => best = Some((parsed, entry)),
                Some((best_version, _)) if parsed > *best_version => {
                    best = Some((parsed, entry));
                }
                _ => {}
            }
        }

        best.map(|(_, entry)| entry)
            .ok_or_else(|| anyhow::anyhow!("No versions found for '{}'", package))
    }
}

pub struct FsRegistry {
    index_path: PathBuf,
}

impl FsRegistry {
    pub fn new(index_path: PathBuf) -> Self {
        Self { index_path }
    }

    pub fn list_packages(&self) -> Result<Vec<String>> {
        let mut packages = Vec::new();
        self.collect_packages(&self.index_path, &mut packages)?;
        packages.sort();
        Ok(packages)
    }

    fn collect_packages(&self, dir: &Path, packages: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(dir)
            .with_context(|| format!("failed to read registry directory {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                self.collect_packages(&path, packages)?;
                continue;
            }

            if path.file_name().and_then(|name| name.to_str()) == Some("config.json") {
                continue;
            }

            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                packages.push(name.to_string());
            }
        }

        Ok(())
    }

    fn package_index_path(&self, package: &str) -> PathBuf {
        self.index_path.join(package)
    }
}

impl Registry for FsRegistry {
    fn get_versions(&self, package: &str) -> Result<Vec<RegistryEntry>> {
        let path = self.package_index_path(package);

        if !path.exists() {
            bail!("Package '{}' not found in registry index", package);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read registry file {:?}", path))?;
        parse_registry_entries(package, &content)
    }
}

pub struct HttpRegistry {
    api_base: String,
    download_base: String,
    client: Client,
}

impl HttpRegistry {
    pub fn new(api_base: impl Into<String>, download_base: impl Into<String>) -> Self {
        Self {
            api_base: normalize_base_url(api_base.into()),
            download_base: normalize_base_url(download_base.into()),
            client: Client::new(),
        }
    }

    pub fn download_url(&self, package: &str, version: &str) -> String {
        format!("{}/{}/{}.tar.gz", self.download_base, package, version)
    }
}

impl Registry for HttpRegistry {
    fn get_versions(&self, package: &str) -> Result<Vec<RegistryEntry>> {
        let url = format!("{}/index/{}", self.api_base, package);
        let response = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("failed to fetch registry index from {}", url))?
            .error_for_status()
            .with_context(|| format!("registry server returned an error for {}", url))?;
        let body = response
            .text()
            .with_context(|| format!("failed to read registry response body from {}", url))?;

        parse_registry_entries(package, &body)
    }
}

fn parse_registry_entries(package: &str, content: &str) -> Result<Vec<RegistryEntry>> {
    let mut entries = Vec::new();

    for (line_number, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let entry: RegistryEntry = serde_json::from_str(trimmed).with_context(|| {
            format!(
                "Invalid registry entry for '{}' on line {}",
                package,
                line_number + 1
            )
        })?;

        entries.push(entry);
    }

    if entries.is_empty() {
        bail!("No versions found for '{}'", package);
    }

    Ok(entries)
}

fn normalize_base_url(base: String) -> String {
    base.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::{FsRegistry, Registry};
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reads_flat_index_file() {
        let fixture = TestFixture::new("reads_flat_index_file");
        fixture.write_index(
            "dora-yolo",
            "{\"name\":\"dora-yolo\",\"vers\":\"0.1.0\",\"deps\":[],\"checksum\":\"abc\"}\n\
             {\"name\":\"dora-yolo\",\"vers\":\"0.2.0\",\"deps\":[],\"checksum\":\"def\"}\n",
        );

        let registry = FsRegistry::new(fixture.root.clone());
        let latest = registry.get_latest_version("dora-yolo").unwrap();

        assert_eq!(latest.name, "dora-yolo");
        assert_eq!(latest.vers, "0.2.0");
    }

    #[test]
    fn lists_packages_from_flat_layout() {
        let fixture = TestFixture::new("lists_packages_from_flat_layout");
        fixture.write_config();
        fixture.write_index(
            "dora-camera",
            "{\"name\":\"dora-camera\",\"vers\":\"0.1.0\",\"deps\":[],\"checksum\":\"xyz\"}\n",
        );
        fixture.write_index(
            "dora-yolo",
            "{\"name\":\"dora-yolo\",\"vers\":\"0.1.0\",\"deps\":[],\"checksum\":\"abc\"}\n",
        );

        let registry = FsRegistry::new(fixture.root.clone());
        let packages = registry.list_packages().unwrap();

        assert_eq!(packages, vec!["dora-camera".to_string(), "dora-yolo".to_string()]);
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
            let root = env::temp_dir().join(format!("registry-client-{test_name}-{unique}"));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn write_config(&self) {
            fs::write(self.root.join("config.json"), "{\"dl\":\"registry/packages\",\"api\":null}\n")
                .unwrap();
        }

        fn write_index(&self, package: &str, body: &str) {
            let path = self.root.join(package);
            fs::write(path, body).unwrap();
        }
    }

    impl Drop for TestFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
