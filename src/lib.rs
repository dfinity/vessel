use anyhow::{self, Context, Result};
use flate2::read::GzDecoder;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, HashMap};
use std::fs;
use std::iter::Iterator;
use std::path::{Path, PathBuf};
use std::process::Command;
use tar::Archive;
use tempfile::TempDir;
use topological_sort::TopologicalSort;
use walkdir::WalkDir;

#[derive(Debug, Default)]
pub struct Vessel {
    pub package_set: PackageSet,
    pub manifest: Manifest,
}

impl Vessel {
    pub fn new(package_set_file: &PathBuf) -> Result<Vessel> {
        let mut new_vessel: Vessel = Default::default();
        new_vessel.read_package_set(package_set_file)?;
        new_vessel.read_manifest_file()?;
        Ok(new_vessel)
    }

    pub fn new_without_manifest(package_set_file: &PathBuf) -> Result<Vessel> {
        let mut new_vessel: Vessel = Default::default();
        new_vessel.read_package_set(package_set_file)?;
        Ok(new_vessel)
    }

    fn read_manifest_file(&mut self) -> Result<()> {
        let manifest_file = PathBuf::from("vessel.dhall");
        self.manifest = serde_dhall::from_file(manifest_file)
            .parse()
            .context("Failed to parse the vessel.dhall file")?;
        Ok(())
    }

    fn read_package_set(&mut self, package_set_file: &PathBuf) -> Result<()> {
        self.package_set = serde_dhall::from_file(package_set_file)
            .parse()
            .context("Failed to parse the package set file")?;
        Ok(())
    }

    /// Installs all transitive dependencies and returns a mapping of package name -> installation location
    pub fn install_packages(&self) -> Result<Vec<(Name, PathBuf)>> {
        let install_plan = self
            .package_set
            .transitive_deps(self.manifest.dependencies.clone());

        info!("Installing {} packages", install_plan.len());

        let paths = install_plan
            .iter()
            .map(|package| download_package(package).map(|path| (package.name.clone(), path)))
            .collect::<Result<Vec<(String, PathBuf)>>>()?;

        info!("Installation complete.");

        Ok(paths)
    }

    /// Verifies that every source file inside the given package compiles in the current package set
    pub fn verify_package(
        &self,
        moc: &PathBuf,
        moc_args: &Option<String>,
        name: &str,
    ) -> Result<()> {
        match self.package_set.find(name) {
            None => Err(anyhow::anyhow!(
                "The package \"{}\" does not exist in the package set",
                name
            )),
            Some(package) => {
                let mut cmd = Command::new(moc);
                cmd.arg("--check");
                if let Some(args) = moc_args {
                    cmd.args(args.split(' '));
                }
                download_package(&package)?;
                let dependencies = self
                    .package_set
                    .transitive_deps(package.dependencies.clone());
                for package in dependencies {
                    let path = download_package(&package)?;
                    cmd.arg("--package").arg(&package.name).arg(path);
                }

                package.sources().for_each(|entry_point| {
                    cmd.arg(entry_point);
                });
                let output = cmd.output()?;
                if output.status.success() {
                    info!("Verified \"{}\"", package.name);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "Failed to verify \"{}\" with:\n{}",
                        package.name,
                        String::from_utf8(output.stderr)?
                    ))
                }
            }
        }
    }

    pub fn verify_all(&self, moc: &PathBuf, moc_args: &Option<String>) -> Result<()> {
        let mut errors: Vec<(Name, anyhow::Error)> = vec![];
        for package in &self.package_set.topo_sorted() {
            if errors
                .iter()
                .find(|(n, _)| package.dependencies.contains(n))
                .is_none()
            {
                if let Err(err) = self.verify_package(moc, moc_args, &package.name) {
                    errors.push((package.name.clone(), err))
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            let err = anyhow::anyhow!(
                "Failed to verify: {:?}",
                errors
                    .iter()
                    .map(|(n, _)| n.clone())
                    .collect::<Vec<String>>()
            );
            for err in errors.iter().rev() {
                eprintln!("{}", err.1);
            }
            Err(err)
        }
    }
}

/// Downloads a package either as a tar-ball from Github or clones it as a repo
pub fn download_package(package: &Package) -> Result<PathBuf> {
    let package_dir = Path::new(".vessel").join(package.name.clone());
    if !package_dir.exists() {
        fs::create_dir_all(&package_dir).context(format!(
            "Failed to create the package directory at {}",
            package_dir.display()
        ))?;
    }
    let repo_dir = package_dir.join(&package.version);
    if !repo_dir.exists() {
        let tmp = Path::new(".vessel").join(".tmp");
        if !tmp.exists() {
            fs::create_dir_all(&tmp)?
        }
        if package.repo.starts_with("https://github.com") {
            info!("Downloading tar-ball: \"{}\"", package.name);
            download_tar_ball(&tmp, &repo_dir, &package.repo, &package.version).or_else(|_| {
                warn!(
                    "Downloading tar-ball failed, cloning as git repo instead: \"{}\"",
                    package.name
                );
                clone_package(&tmp, &repo_dir, &package.repo, &package.version)
            })?
        } else {
            info!("Cloning git repository: \"{}\"", package.name);
            clone_package(&tmp, &repo_dir, &package.repo, &package.version)?
        }
    } else {
        debug!(
            "{} at version {} has already been downloaded",
            package.name, package.version
        )
    }
    Ok(repo_dir.join("src"))
}

/// Downloads and unpacks a tar-ball from Github into the `dest` path
fn download_tar_ball(tmp: &Path, dest: &Path, repo: &str, version: &str) -> Result<()> {
    let target = format!(
        "{}/archive/{}/.tar.gz",
        repo.trim_end_matches(".git"),
        version
    );
    let response = reqwest::blocking::get(&target)?;

    // We unpack into a temporary directory and rename it in one go once
    // the full unpacking was successful
    let tmp_dir: TempDir = tempfile::tempdir_in(tmp)?;
    Archive::new(GzDecoder::new(response)).unpack(tmp_dir.path())?;

    // We expect an unpacked repo to contain exactly one directory
    let repo_dir = match fs::read_dir(tmp_dir.path())?.next() {
        None => return Err(anyhow::anyhow!("Unpacked an empty tarball for {}", repo)),
        Some(dir) => dir?,
    };

    if !repo_dir.path().is_dir() {
        return Err(anyhow::anyhow!("Failed to unpack tarball for \"{}\"", repo));
    }
    fs::rename(repo_dir.path(), dest)?;

    Ok(())
}

/// Clones `repo` into `dest` and checks out `version`
fn clone_package(tmp: &Path, dest: &Path, repo: &str, version: &str) -> Result<()> {
    let tmp_dir: TempDir = tempfile::tempdir_in(tmp)?;
    Command::new("git")
        .args(&["clone", repo, "repo"])
        .current_dir(tmp_dir.path())
        .output()
        .context(format!("Failed to clone the repo at {}", repo))?;
    let repo_dir = tmp_dir.path().join("repo");
    // TODO: fail when checkout fails (status code != 0)
    Command::new("git")
        .args(&["-c", "advice.detachedHead=false", "checkout", version])
        .current_dir(&repo_dir)
        .output()
        .context(format!(
            "Failed to checkout version {} for the repository {} in {}",
            version,
            repo,
            repo_dir.display()
        ))?;
    fs::rename(repo_dir, dest)?;
    Ok(())
}

/// TODO: Initializes a new vessel project by creating a `vessel.dhall` file with no
/// dependencies and adding a dummy package set (for now, we should pull this
/// from a community maintained repository instead)
pub fn init() -> Result<()> {
    let package_set_path: PathBuf = PathBuf::from("package-set.dhall");
    let manifest_path: PathBuf = PathBuf::from("vessel.dhall");
    if package_set_path.exists() {
        return Err(anyhow::anyhow!(
            "Failed to initialize, there is an existing package-set.dhall file here"
        ));
    }
    if manifest_path.exists() {
        return Err(anyhow::anyhow!(
            "Failed to initialize, there is an existing vessel.dhall file here"
        ));
    }
    // "TODO: Implement creation of the dhall files
    Ok(())
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
pub struct PackageInfo {
    pub repo: Url,
    pub version: Tag,
    pub dependencies: Vec<Name>,
}

impl Package {
    pub fn install_path(&self) -> PathBuf {
        Path::new(".vessel")
            .join(self.name.clone())
            .join(self.version.clone())
            .join("src")
    }

    /// Returns all Motoko sources found inside this package's installation directory
    pub fn sources(&self) -> impl Iterator<Item = PathBuf> {
        WalkDir::new(self.install_path())
            .into_iter()
            .filter_map(|e| match e {
                Err(_) => None,
                Ok(entry) => {
                    let file_name = entry.path();
                    if let Some(ext) = file_name.extension() {
                        if ext == "mo" {
                            return Some(file_name.to_owned());
                        }
                    }
                    None
                }
            })
    }
}

#[serde(from = "NewPackageSet")]
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PackageSet(pub Vec<Package>);

#[serde(transparent)]
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NewPackageSet(pub HashMap<Name, PackageInfo>);

#[derive(Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub dependencies: Vec<Name>,
}

impl From<NewPackageSet> for PackageSet {
    fn from(package_set: NewPackageSet) -> PackageSet {
        let packages = package_set.0.into_iter().map(|(name, info)| Package {
            name,
            repo: info.repo,
            version: info.version,
            dependencies: info.dependencies,
        }).collect();
        PackageSet(packages)
    }
}

impl PackageSet {
    /// Finds a package by name
    fn find(&self, name: &str) -> Option<&Package> {
        self.0.iter().find(|p| p.name == *name)
    }

    fn find_unsafe(&self, name: &str) -> &Package {
        self.find(name)
            .unwrap_or_else(|| panic!("Package \"{}\" wasn't specified in the package set", name))
    }

    /// Finds all transitive dependencies starting from the given package names.
    /// Includes the entry points in the resulting vector
    fn transitive_deps(&self, entry_points: Vec<Name>) -> Vec<&Package> {
        let mut found: HashSet<Name> = HashSet::new();
        let mut todo: Vec<Name> = entry_points;
        while let Some(next) = todo.pop() {
            if !found.contains(&next) {
                todo.append(&mut self.find_unsafe(&next).dependencies.clone());
                found.insert(next);
            }
        }
        // Once we have incremental compilation we could return these toposorted to allow
        // starting to compile the first packages while others are still being downloaded.
        // For now we sort them to get deterministic behaviour for testing.
        let mut found: Vec<Name> = found.into_iter().collect();
        found.sort();
        found.iter().map(|n| self.find_unsafe(n)).collect()
    }

    pub fn topo_sorted(&self) -> Vec<&Package> {
        let mut ts = TopologicalSort::<&str>::new();
        for package in &self.0 {
            ts.insert(package.name.as_ref());
            for dep in &package.dependencies {
                ts.add_dependency(dep.as_ref(), package.name.as_ref())
            }
        }
        ts.map(|name| self.find_unsafe(name)).collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn mk_package(name: &str, deps: Vec<&str>) -> Package {
        Package {
            name: name.to_string(),
            repo: "".to_string(),
            version: "".to_string(),
            dependencies: deps.into_iter().map(|x| x.to_string()).collect(),
        }
    }

    #[test]
    fn it_finds_a_transitive_dependency() {
        let a = mk_package("A", vec!["B"]);
        let b = mk_package("B", vec![]);
        let ps = PackageSet(vec![a.clone(), b.clone()]);
        assert_eq!(vec![&b], ps.transitive_deps(vec!["B".to_string()]));
        assert_eq!(vec![&a, &b], ps.transitive_deps(vec!["A".to_string()]))
    }

    #[test]
    fn it_finds_transitive_dependencies_with_overlaps() {
        let a = mk_package("A", vec!["B"]);
        let b = mk_package("B", vec![]);
        let c = mk_package("C", vec!["B"]);
        let ps = PackageSet(vec![a.clone(), b.clone(), c.clone()]);
        assert_eq!(
            vec![&a, &b, &c],
            ps.transitive_deps(vec!["A".to_string(), "C".to_string()])
        );

        assert_eq!(vec![&b, &c], ps.transitive_deps(vec!["C".to_string()]))
    }
}
