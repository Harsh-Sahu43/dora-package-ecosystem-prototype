pub mod installer;
pub mod manifest;
pub mod publish;
pub mod resolver;

use anyhow::Result;
use installer::InstallOutcome;

use registry_client::default_registry;
use resolver::Resolver;
use std::path::Path;

pub fn install(node: &str) -> Result<()> {
    let registry = default_registry()?;
    let resolver = Resolver::new(registry);
    let graph = resolver.resolve(node)?;

    for entry in graph.install_plan() {
        println!("Installing {} {}", entry.name, entry.vers);

        match installer::install_node(&entry.name, &entry.vers)? {
            InstallOutcome::Installed => {
                println!("Installed {} {}", entry.name, entry.vers);
            }
            InstallOutcome::Cached => {
                println!("Using cached {} {}", entry.name, entry.vers);
            }
        }
    }

    Ok(())
}

pub fn publish(package_dir: &Path) -> Result<()> {
    publish::publish(package_dir)
}
