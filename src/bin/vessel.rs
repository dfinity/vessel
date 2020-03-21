use vessel;

use anyhow::Result;
use pretty_env_logger;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(about = "Simple package management for Motoko")]
struct Opts {
    /// Which file to read the package set from
    #[structopt(long, parse(from_os_str), default_value = "package-set.json")]
    package_set: PathBuf,
    /// Which file to read as the manifest file
    #[structopt(long, parse(from_os_str), default_value = "vessel.json")]
    manifest: PathBuf,
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Sets up the minimal project configuration
    Init,
    /// Installs all dependencies and prints a human readable summary
    Install,
    /// Installs all dependencies and outputs the package flags to be passed on
    /// to the Motoko compiler tools
    Sources,
}

fn main() -> Result<()> {
    pretty_env_logger::init();
    let opts = Opts::from_args();
    match opts.command {
        Command::Init => vessel::init(),
        Command::Install => {
            let vessel = vessel::Vessel::new(true, &opts.package_set, &opts.manifest)?;
            let _ = vessel.install_packages()?;
            Ok(())
        }
        Command::Sources => {
            let vessel = vessel::Vessel::new(false, &opts.package_set, &opts.manifest)?;
            let sources = vessel
                .install_packages()?
                .into_iter()
                .map(|(name, path)| format!("--package {} {}", name, path.display().to_string()))
                .collect::<Vec<_>>()
                .join(" ");
            print!("{}", sources);
            Ok(())
        }
    }
}
