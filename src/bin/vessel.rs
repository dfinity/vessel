use vessel;

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
    Install {
        #[structopt(long)]
        list_packages: bool,
    },
    Build {
        #[structopt(parse(from_os_str))]
        entry_point: PathBuf,
    },
}

fn main() {
    pretty_env_logger::init();
    let opts = Opts::from_args();
    match opts.command {
        Command::Install { list_packages } => {
            let vessel =
                vessel::Vessel::new(!list_packages, &opts.package_set, &opts.manifest).unwrap();
            let packages = vessel.install_packages().unwrap();
            if list_packages {
                for (name, path) in packages {
                    println!("--package {} {}", name, path.display().to_string())
                }
            }
        }
        Command::Build { entry_point } => {
            let vessel = vessel::Vessel::new(true, &opts.package_set, &opts.manifest).unwrap();
            let packages = vessel.install_packages().unwrap();
            vessel.build_module(entry_point, packages).unwrap();
        }
    }
}
