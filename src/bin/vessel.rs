use vessel;

use anyhow::Result;
use pretty_env_logger;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(about = "Simple package management for Motoko")]
struct Opts {
    #[structopt(long, parse(from_os_str), default_value = "package-set.json")]
    package_set: PathBuf,
    #[structopt(long, parse(from_os_str), default_value = "vessel.json")]
    manifest: PathBuf,
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    Init,
    Install {
        #[structopt(long)]
        list_packages: bool,
    },
    Build {
        #[structopt(parse(from_os_str))]
        entry_point: PathBuf,
    },
}

fn main() -> Result<()> {
    pretty_env_logger::init();
    let opts = Opts::from_args();
    match opts.command {
        Command::Init => vessel::init(),
        Command::Install { list_packages } => {
            let vessel = vessel::Vessel::new(!list_packages, &opts.package_set, &opts.manifest)?;
            let packages = vessel.install_packages()?;
            if list_packages {
                for (name, path) in packages {
                    println!("--package {} {}", name, path.display().to_string())
                }
            }
            Ok(())
        }
        Command::Build { entry_point } => {
            let vessel = vessel::Vessel::new(true, &opts.package_set, &opts.manifest)?;
            let packages = vessel.install_packages()?;
            vessel.build_module(entry_point, packages)
        }
    }
}
