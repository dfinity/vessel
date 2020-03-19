use colored::*;
use flate2::read::GzDecoder;
use log::debug;
use reqwest;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tar::Archive;
use tempfile::TempDir;

pub struct Vessel {
    pub output_for_humans: bool,
    pub package_set: PackageSet,
    pub manifest: Manifest,
}

impl Vessel {
    pub fn new(
        output_for_humans: bool,
        package_set_file: &PathBuf,
        manifest_file: &PathBuf,
    ) -> Result<Vessel, Box<dyn std::error::Error>> {
        let package_set: PackageSet = serde_json::from_reader(fs::File::open(package_set_file)?)?;
        let manifest: Manifest = serde_json::from_reader(fs::File::open(manifest_file)?)?;
        Ok(Vessel {
            output_for_humans,
            package_set,
            manifest,
        })
    }

    pub fn for_humans<F>(&self, s: F)
    where
        F: FnOnce(),
    {
        if self.output_for_humans {
            s()
        }
    }

    pub fn install_packages(&self) -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
        let install_plan = self
            .package_set
            .transitive_deps(self.manifest.dependencies.clone());

        self.for_humans(|| {
            println!(
                "{} Installing {} packages",
                "[Info]".blue(),
                install_plan.len()
            )
        });
        for package in &install_plan {
            self.download_package(package)?
        }
        self.for_humans(|| println!("{} Installation complete.", "[Info]".blue()));

        Ok(install_plan
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    PathBuf::from(&format!(".vessel/{}/{}/src", p.name, p.version)),
                )
            })
            .collect())
    }

    pub fn download_package(&self, package: &Package) -> Result<(), Box<dyn std::error::Error>> {
        let package_dir = format!(".vessel/{}", package.name);
        let package_dir = Path::new(&package_dir);
        if !package_dir.exists() {
            fs::create_dir_all(package_dir)?;
        }
        let repo_dir = package_dir.join(&package.version);
        if !repo_dir.exists() {
            if package.repo.starts_with("https://github.com") {
                self.for_humans(|| {
                    println!(
                        "{} Downloading tar-ball: \"{}\"",
                        "[Info]".blue(),
                        package.name
                    )
                });
                download_tar_ball(&repo_dir, &package.repo, &package.version).or_else(|_| {
                    self.for_humans(|| {
                        println!(
                            "{} Downloading tar-ball failed, cloning as git repo instead: \"{}\"",
                            "[Warn]".yellow(),
                            package.name
                        )
                    });
                    clone_package(&repo_dir, &package.repo, &package.version)
                })?
            } else {
                self.for_humans(|| {
                    println!(
                        "{} Cloning git repository: \"{}\"",
                        "[Info]".blue(),
                        package.name
                    )
                });
                clone_package(&repo_dir, &package.repo, &package.version)?
            }
        } else {
            debug!(
                "{} at version {} has already been downloaded",
                package.name, package.version
            )
        }
        Ok(())
    }

    pub fn build_module(
        &self,
        entry_point: PathBuf,
        packages: Vec<(String, PathBuf)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut package_flags = vec![
            "-wasi-system-api".to_string(),
            entry_point.display().to_string(),
        ];
        for (name, path) in packages {
            package_flags.push("--package".to_string());
            package_flags.push(name);
            package_flags.push(path.display().to_string());
        }

        let mut moc_command = Command::new("moc");
        let moc_command = moc_command.args(&package_flags);

        debug!("About to run: {:?}", moc_command);
        let output = moc_command.output()?;
        if output.status.success() {
            self.for_humans(|| println!("{} Build successful.", "[Info]".blue()))
        } else {
            eprintln!("{} Build failed with:", "[Error]".red());
            io::stdout().write_all(&output.stdout).unwrap();
            io::stderr().write_all(&output.stderr).unwrap();
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum Error {
    MalformedRepo(PathBuf),
    EmptyArchive(PathBuf),
}

impl std::error::Error for Error {}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::MalformedRepo(dir) => write!(
                f,
                "[ERROR] {} should've been an unpacked repository, but wasn't",
                dir.display()
            ),
            Error::EmptyArchive(dir) => write!(
                f,
                "[ERROR] Downloaded an empty archive to: {}",
                dir.display()
            ),
        }
    }
}

pub type Url = String;
pub type Tag = String;
pub type Name = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Package {
    pub name: Name,
    pub repo: Url,
    pub version: Tag,
    pub dependencies: Vec<Name>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageSet(pub Vec<Package>);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub dependencies: Vec<Name>,
}

impl PackageSet {
    fn find(&self, name: &Name) -> &Package {
        self.0.iter().find(|p| p.name == *name).expect(&format!(
            "Package \"{}\" wasn't specified in the package set",
            name
        ))
    }

    fn transitive_deps(&self, entry_points: Vec<Name>) -> Vec<&Package> {
        let mut found: HashSet<Name> = HashSet::new();
        let mut todo: Vec<Name> = entry_points;
        while let Some(next) = todo.pop() {
            if !found.contains(&next) {
                todo.append(&mut self.find(&next).dependencies.clone());
                found.insert(next);
            }
        }
        found.iter().map(|n| self.find(n)).collect()
    }
}

fn download_tar_ball(
    dest: &Path,
    repo: &str,
    version: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let target = format!(
        "{}/archive/{}/.tar.gz",
        repo.trim_end_matches(".git"),
        version
    );
    let response = reqwest::blocking::get(&target)?;

    // We unpack into a temporary directory and rename it in one go once
    // the full unpacking was successful
    let tmp_dir: TempDir = tempfile::tempdir()?;
    Archive::new(GzDecoder::new(response)).unpack(tmp_dir.path())?;

    // We expect an unpacked repo to contain exactly one directory
    let repo_dir = match fs::read_dir(tmp_dir.path())?.next() {
        None => return Err(Box::new(Error::EmptyArchive(tmp_dir.path().to_owned()))),
        Some(dir) => dir?,
    };

    if !repo_dir.path().is_dir() {
        return Err(Box::new(Error::MalformedRepo(repo_dir.path())));
    }
    fs::rename(repo_dir.path(), dest)?;

    Ok(())
}

fn clone_package(dest: &Path, repo: &str, version: &str) -> Result<(), Box<dyn std::error::Error>> {
    let tmp_dir: TempDir = tempfile::tempdir()?;
    Command::new("git")
        .args(&["clone", repo, "repo"])
        .current_dir(tmp_dir.path())
        .output()
        .expect(&format!("Failed to clone the repo at {}", repo));
    let repo_dir = tmp_dir.path().join("repo");
    Command::new("git")
        .args(&["-c", "advice.detachedHead=false", "checkout", version])
        .current_dir(&repo_dir)
        .output()
        .expect(&format!(
            "Failed to checkout version {} for the repository {} in {}",
            version,
            repo,
            repo_dir.display()
        ));
    fs::rename(repo_dir, dest)?;
    Ok(())
}
