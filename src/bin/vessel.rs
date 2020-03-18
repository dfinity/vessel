use vessel;

use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(about = "Simple package management for Motoko")]
struct Opts {
    #[structopt(long, parse(from_os_str), default_value = "package-set.json")]
    package_set: PathBuf,
    #[structopt(long, parse(from_os_str), default_value = "manifest.json")]
    manifest: PathBuf,
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    Install,
    Build {
        #[structopt(parse(from_os_str))]
        entry_point: PathBuf,
    },
}

fn main() {
    let opts = Opts::from_args();
    match opts.command {
        Command::Install => {
            vessel::install_packages(&opts.package_set, &opts.manifest);
        }
        Command::Build { entry_point } => {
            let package_flags = vessel::install_packages(&opts.package_set, &opts.manifest);

            println!("Running: moc {} {}", package_flags, entry_point.display())
        }
    }
}
