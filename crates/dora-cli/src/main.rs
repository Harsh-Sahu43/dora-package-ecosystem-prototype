use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dora")]
#[command(about = "DORA package manager prototype")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    node: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    Install { node: String },
    Publish { path: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match (cli.command, cli.node) {
        (Some(Commands::Install { node }), None) => {
            package_manager::install(&node)?;
        }
        (Some(Commands::Publish { path }), None) => {
            package_manager::publish(std::path::Path::new(&path))?;
        }
        (None, Some(node)) => {
            package_manager::install(&node)?;
        }
        (Some(Commands::Install { .. }), Some(_)) => {
            anyhow::bail!(
                "provide either `dora-cli <node>` or `dora-cli install <node>`, not both"
            );
        }
        (Some(Commands::Publish { .. }), Some(_)) => {
            anyhow::bail!("publish uses only `dora-cli publish <package-path>`");
        }
        (None, None) => {
            anyhow::bail!(
                "missing command. usage: `dora-cli <node>`, `dora-cli install <node>`, or `dora-cli publish <package-path>`"
            );
        }
    }

    Ok(())
}

// use registry_client::registry::RegistryClient;
// use std::path::PathBuf;

// fn main() {
//     let registry = RegistryClient::new(
//         PathBuf::from("registry/registry-index/nodes")
//     );

//     let meta = registry.get_node_metadata("dora-yolo").unwrap();

//     println!("{:#?}", meta);
// }

// use registry_client::registry::RegistryClient;
// use package_manager::resolver::Resolver;
// use std::path::PathBuf;

// fn main() {
//     let registry = RegistryClient::new(
//         PathBuf::from("registry/registry-index/nodes")
//     );

//     let resolver = Resolver::new(registry);

//     let graph = resolver.resolve("dora-yolo").unwrap();

//     println!("{:#?}", graph);
// }
