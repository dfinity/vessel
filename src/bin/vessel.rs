use vessel;

use std::path::PathBuf;
use std::process;
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
    let opts = Opts::from_args();
    match opts.command {
        Command::Install { list_packages } => {
            let packages = vessel::install_packages(&opts.package_set, &opts.manifest);
            if list_packages {
                for (name, path) in packages {
                    println!("--package {} {}", name, path.display().to_string())
                }
            }
        }
        Command::Build { entry_point } => {
            let packages = vessel::install_packages(&opts.package_set, &opts.manifest);

            let mut package_flags = vec![
                "-wasi-system-api".to_string(),
                entry_point.display().to_string(),
            ];
            for (name, path) in packages {
                package_flags.push("--package".to_string());
                package_flags.push(name);
                package_flags.push(path.display().to_string());
            }

            let mut moc_command = process::Command::new("moc");
            let moc_command = moc_command.args(&package_flags);

            println!("About to run: {:?}", moc_command);
            moc_command.spawn().expect("Failed to start moc");
        }
    }
}
