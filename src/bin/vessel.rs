use anyhow::Result;
use fern::colors::ColoredLevelConfig;
use fern::Output;
use log::LevelFilter;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(about = "Simple package management for Motoko")]
struct Opts {
    /// Which file to read the package set from
    #[structopt(long, parse(from_os_str), default_value = "package-set.dhall")]
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

fn setup_logger(opts: &Opts) -> Result<(), fern::InitError> {
    let (log_level, out_channel): (LevelFilter, Output) = match opts.command {
        Command::Sources => (log::LevelFilter::Error, std::io::stderr().into()),
        _ => (log::LevelFilter::Info, std::io::stdout().into()),
    };
    let colors = ColoredLevelConfig::new();
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}] {}",
                colors.color(record.level()),
                message
            ))
        })
        .level(log_level)
        .chain(out_channel)
        .apply()?;
    Ok(())
}

fn main() -> Result<()> {
    let opts = Opts::from_args();
    setup_logger(&opts)?;

    match opts.command {
        Command::Init => vessel::init(),
        Command::Install => {
            let vessel = vessel::Vessel::new(&opts.package_set)?;
            let _ = vessel.install_packages()?;
            Ok(())
        }
        Command::Sources => {
            let vessel = vessel::Vessel::new(&opts.package_set)?;
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
            let vessel = vessel::Vessel::new_without_manifest(&opts.package_set)?;
            match package {
                None => vessel.verify_all(&moc, &moc_args),
                Some(package) => vessel.verify_package(&moc, &moc_args, &package),
            }
        }
    }
}
