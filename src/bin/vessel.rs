use anyhow::Result;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(about = "Simple package management for Motoko")]
struct Opts {
    /// Which file to read the package set from
    #[structopt(long, parse(from_os_str), default_value = "package-set.json")]
    package_set: PathBuf,
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
    /// Verifies that every package in the package set builds successfully
    Verify {
        /// Path to the `moc` binary
        #[structopt(long, parse(from_os_str), default_value = "moc")]
        moc: PathBuf,
        /// Additional arguments to pass to `moc` when checking packages
        #[structopt(long)]
        moc_args: Option<String>,
        /// When specified only verified the given package name
        #[structopt()]
        package: Option<String>,
    },
}

fn main() -> Result<()> {
    pretty_env_logger::init();
    let opts = Opts::from_args();
    match opts.command {
        Command::Init => vessel::init(),
        Command::Install => {
            let vessel = vessel::Vessel::new(true, &opts.package_set)?;
            let _ = vessel.install_packages()?;
            Ok(())
        }
        Command::Sources => {
            let vessel = vessel::Vessel::new(false, &opts.package_set)?;
            let sources = vessel
                .install_packages()?
                .into_iter()
                .map(|(name, path)| format!("--package {} {}", name, path.display().to_string()))
                .collect::<Vec<_>>()
                .join(" ");
            print!("{}", sources);
            Ok(())
        }
        Command::Verify {
            moc,
            moc_args,
            package,
        } => {
            let vessel = vessel::Vessel::new_without_manifest(true, &opts.package_set)?;
            match package {
                None => vessel.verify_all(&moc, &moc_args),
                Some(package) => vessel.verify_package(&moc, &moc_args, &package),
            }
        }
    }
}
