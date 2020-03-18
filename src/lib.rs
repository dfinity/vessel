use flate2::read::GzDecoder;
use reqwest;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use tar::Archive;
use tempfile::TempDir;

pub fn install_packages(package_set: &PathBuf, manifest: &PathBuf) -> String {
    let package_set: PackageSet =
        serde_json::from_reader(fs::File::open(package_set).unwrap()).unwrap();
    let manifest: Manifest = serde_json::from_reader(fs::File::open(manifest).unwrap()).unwrap();
    let install_plan = package_set.transitive_deps(manifest.dependencies);

    println!(
        "Install plan: {}",
        install_plan
            .iter()
            .map(|p| p.name.as_ref())
            .collect::<Vec<_>>()
            .join(", ")
    );

    for package in &install_plan {
        download_package(package).unwrap()
    }
    println!("Finished installing.");

    install_plan
        .iter()
        .map(|p| {
            format!(
                "--package {} .packages/{}/{}/src",
                p.name, p.name, p.version
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn download_package(package: &Package) -> Result<(), Box<dyn std::error::Error>> {
    let package_dir = format!(".packages/{}", package.name);
    let package_dir = Path::new(&package_dir);
    if !package_dir.exists() {
        fs::create_dir_all(package_dir)?;
    }
    let repo_dir = package_dir.join(&package.version);
    if !repo_dir.exists() {
        println!("Downloading package: {}", package.name);
        download_tar_ball(&repo_dir, &package.repo, &package.version)?
    }
    Ok(())
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

type Url = String;
type Tag = String;
type Name = String;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Package {
    name: Name,
    repo: Url,
    version: Tag,
    dependencies: Vec<Name>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct PackageSet(Vec<Package>);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Manifest {
    dependencies: Vec<Name>,
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
