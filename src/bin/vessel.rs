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
    Install,
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
            for (name, path) in vessel.install_packages()? {
                print!(" --package {} {}", name, path.display().to_string())
            }
            Ok(())
        }
    }
}
