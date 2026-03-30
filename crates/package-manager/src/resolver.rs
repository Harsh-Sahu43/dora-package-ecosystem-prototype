use anyhow::Result;
use registry_client::{Registry, RegistryEntry};
use semver::{Version, VersionReq};
use std::collections::HashMap;

#[derive(Debug)]
pub struct DependencyGraph {
    pub nodes: HashMap<String, RegistryEntry>,
    install_order: Vec<String>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            install_order: Vec::new(),
        }
    }

    pub fn add_node(&mut self, entry: RegistryEntry) {
        let name = entry.name.clone();
        self.nodes.insert(name.clone(), entry);
        self.install_order.push(name);
    }

    pub fn install_plan(&self) -> Vec<&RegistryEntry> {
        self.install_order
            .iter()
            .filter_map(|name| self.nodes.get(name))
            .collect()
    }
}

pub struct Resolver<R: Registry> {
    registry: R,
}

impl<R: Registry> Resolver<R> {
    pub fn new(registry: R) -> Self {
        Self { registry }
    }

    pub fn resolve(&self, root: &str) -> Result<DependencyGraph> {
        let mut graph = DependencyGraph::new();
        let entry = self.registry.get_latest_version(root)?;
        self.resolve_entry(entry, &mut graph)?;
        Ok(graph)
    }

    fn resolve_entry(&self, entry: RegistryEntry, graph: &mut DependencyGraph) -> Result<()> {
        if graph.nodes.contains_key(&entry.name) {
            return Ok(());
        }

        for dependency in &entry.deps {
            let resolved = self.resolve_dependency(&dependency.name, &dependency.req)?;
            self.resolve_entry(resolved, graph)?;
        }

        graph.add_node(entry);
        Ok(())
    }

    fn resolve_dependency(&self, package: &str, requirement: &str) -> Result<RegistryEntry> {
        let req = VersionReq::parse(requirement)?;
        let versions = self.registry.get_versions(package)?;

        let mut best: Option<(Version, RegistryEntry)> = None;

        for entry in versions {
            let version = Version::parse(&entry.vers)?;
            if !req.matches(&version) {
                continue;
            }

            match &best {
                None => best = Some((version, entry)),
                Some((best_version, _)) if version > *best_version => {
                    best = Some((version, entry));
                }
                _ => {}
            }
        }

        best.map(|(_, entry)| entry).ok_or_else(|| {
            anyhow::anyhow!(
                "no version of '{}' satisfies dependency requirement '{}'",
                package,
                requirement
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Resolver;
    use anyhow::Result;
    use registry_client::{Dependency, Registry, RegistryEntry};
    use std::collections::HashMap;

    struct InMemoryRegistry {
        packages: HashMap<String, Vec<RegistryEntry>>,
    }

    impl InMemoryRegistry {
        fn new(packages: HashMap<String, Vec<RegistryEntry>>) -> Self {
            Self { packages }
        }
    }

    impl Registry for InMemoryRegistry {
        fn get_versions(&self, package: &str) -> Result<Vec<RegistryEntry>> {
            self.packages
                .get(package)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("package '{}' not found", package))
        }
    }

    #[test]
    fn resolves_dependencies_in_install_order() {
        let mut packages = HashMap::new();
        packages.insert(
            "dora-yolo".to_string(),
            vec![RegistryEntry {
                name: "dora-yolo".to_string(),
                vers: "0.1.0".to_string(),
                deps: vec![Dependency {
                    name: "dora-camera".to_string(),
                    req: "^0.1.0".to_string(),
                }],
                checksum: "yolo".to_string(),
            }],
        );
        packages.insert(
            "dora-camera".to_string(),
            vec![RegistryEntry {
                name: "dora-camera".to_string(),
                vers: "0.1.0".to_string(),
                deps: vec![],
                checksum: "camera".to_string(),
            }],
        );

        let resolver = Resolver::new(InMemoryRegistry::new(packages));
        let graph = resolver.resolve("dora-yolo").unwrap();
        let plan = graph
            .install_plan()
            .into_iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();

        assert_eq!(plan, vec!["dora-camera".to_string(), "dora-yolo".to_string()]);
    }
}
