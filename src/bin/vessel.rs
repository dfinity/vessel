use anyhow::Result;
use fern::colors::ColoredLevelConfig;
use fern::Output;
use log::LevelFilter;
use std::io::Write;
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
    Install {
        #[structopt(short = "f")]
        force: bool,
    },
    /// Outputs the import and hash for the latest vessel-package-set release.
    UpgradeSet {
        /// Use this tag instead of latest
        tag: Option<String>,
    },
    /// Installs all dependencies and outputs the package flags to be passed on
    /// to the Motoko compiler tools
    Sources,
    /// Installs the compiler binaries and outputs a path to them
    Bin,
    /// Verifies that every package in the package set builds successfully
    Verify {
        /// The version of the motoko compiler to use. Mutually exclusive with
        /// the `moc` flag.
        #[structopt(long)]
        version: Option<String>,

        /// Path to the `moc` binary. Mutually exclusive with the `version flag`
        #[structopt(long, parse(from_os_str))]
        moc: Option<PathBuf>,

        /// Additional arguments to pass to `moc` when checking packages
        #[structopt(long)]
        moc_args: Option<String>,

        /// When specified only verify the given package name
        #[structopt()]
        package: Option<String>,
    },
}

fn setup_logger(opts: &Opts) -> Result<(), fern::InitError> {
    let (log_level, out_channel): (LevelFilter, Output) = match opts.command {
        Command::Sources | Command::Bin => (log::LevelFilter::Info, std::io::stderr().into()),
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
        Command::Install {force} => {
            let vessel = vessel::Vessel::new(&opts.package_set)?;
            let _ = vessel.install_packages(force)?;
            Ok(())
        }
        Command::UpgradeSet { tag } => {
            let (url, hash) = match tag {
                None => vessel::fetch_latest_package_set()?,
                Some(tag) => vessel::fetch_package_set(&tag)?,
            };
            println!("let upstream =\n      {} {}", url, hash);
            Ok(())
        }
        Command::Bin => {
            let vessel = vessel::Vessel::new(&opts.package_set)?;
            let path = vessel.install_compiler()?;
            print!("{}", path.display());
            std::io::stdout().flush()?;
            Ok(())
        }
        Command::Sources => {
            let vessel = vessel::Vessel::new(&opts.package_set)?;
            let sources = vessel
                .install_packages(false)?
                .into_iter()
                .map(|(name, path)| format!("--package {} {}", name, path.display()))
                .collect::<Vec<_>>()
                .join(" ");
            print!("{}", sources);
            std::io::stdout().flush()?;
            Ok(())
        }
        Command::Verify {
            moc,
            moc_args,
            version,
            package,
        } => {
            let vessel = vessel::Vessel::new_without_manifest(&opts.package_set)?;
            let moc = match (moc, version) {
                (None, None) => PathBuf::from("moc"),
                (Some(moc), None) => moc,
                (None, Some(version)) => {
                    let bin_path = vessel::download_compiler(&version)?;
                    bin_path.join("moc")
                }
                (Some(_), Some(_)) => {
                    return Err(anyhow::anyhow!(
                        "The --version and --moc flags are mutually exclusive."
                    ))
                }
            };
            match package {
                None => vessel.verify_all(&moc, &moc_args),
                Some(package) => vessel.verify_package(&moc, &moc_args, &package),
            }
        }
    }
}
