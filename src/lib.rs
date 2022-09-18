use anyhow::{Context, Result, anyhow, Error};
use flate2::read::GzDecoder;
use log::{debug, info, warn};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{cfg};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;
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
    /// How many parent directories are we nested underneath the project root
    pub nested: u32,
}

impl Vessel {
    fn find_dominating_manifest() -> Result<Option<u32>> {
        let cwd = env::current_dir().context("Unable to access the current directory")?;
        for (depth, path) in cwd.ancestors().enumerate() {
            if path.join("vessel.dhall").exists() || path.join("vessel.mo").exists() {
                if depth != 0 {
                    info!("Changing working directory to {}", path.display());
                    env::set_current_dir(&path).context(format!(
                        "Failed to change current directory to {}",
                        path.display()
                    ))?;
                }
                return Ok(Some(depth as u32));
            }
        }
        Ok(None)
    }

    pub fn new(package_set_file: &Path) -> Result<Vessel> {
        let mut new_vessel = match Vessel::find_dominating_manifest()? {
            None => {
                return Err(anyhow!(
                    "Could not find a 'vessel.dhall' or 'vessel.mo' file in this directory or a parent one."
                ))
            }
            Some(nested) => Vessel {
                nested,
                ..Default::default()
            },
        };
        new_vessel.read_package_set(package_set_file)?;
        new_vessel.read_manifest_file()?;
        Ok(new_vessel)
    }

    pub fn new_without_manifest(package_set_file: &Path) -> Result<Vessel> {
        let mut new_vessel: Vessel = Default::default();
        new_vessel.read_package_set(package_set_file)?;
        Ok(new_vessel)
    }

    fn read_manifest_file(&mut self) -> Result<()> {
        let mo_file = PathBuf::from("vessel.mo");
        self.manifest = if mo_file.exists() {
            motoko::vm::eval_into(&fs::read_to_string(mo_file)?)
            .map_err(|e| anyhow!("Error while reading Motoko config file: {:?}", e))?
        } else {
            let dhall_file = PathBuf::from("vessel.dhall");
            serde_dhall::from_file(dhall_file)
                .static_type_annotation()
                .parse()
                .context("Failed to parse the vessel.dhall file")?
        };
        Ok(())
    }

    fn read_package_set(&mut self, package_set_file: &Path) -> Result<()> {
        self.package_set = PackageSet::new(
            serde_dhall::from_file(package_set_file)
                .static_type_annotation()
                .parse()
                .context("Failed to parse the package set file")?,
        );
        Ok(())
    }

    fn nested_path(&self, path: PathBuf) -> PathBuf {
        if self.nested == 0 {
            return path;
        }

        let mut res = PathBuf::new();
        for _ in 0..self.nested {
            res.push("..");
        }
        res.join(path)
    }

    /// Installs all transitive dependencies and returns a mapping of package name -> installation location
    pub fn install_packages(&self, force: bool) -> Result<Vec<(Name, PathBuf)>> {
        let install_plan = self
            .package_set
            .transitive_deps(self.manifest.dependencies.clone());

        info!("Installing {} packages", install_plan.len());

        let paths = install_plan
            .iter()
            .map(|package| {
                download_package(package, force)
                    .map(|path| (package.name.clone(), self.nested_path(path)))
            })
            .collect::<Result<Vec<(String, PathBuf)>>>()?;

        info!("Installation complete.");

        Ok(paths)
    }

    /// Downloads the compiler binaries at the version specified in the manifest
    /// and returns the path to them.
    pub fn install_compiler(&self) -> Result<PathBuf> {
        let version =
            self.manifest.compiler.as_ref().ok_or_else(|| {
                anyhow!("No compiler version was specified in vessel.dhall")
            })?;
        download_compiler(version).map(|path| self.nested_path(path))
    }

    /// Verifies that every source file inside the given package compiles in the current package set
    pub fn verify_package(&self, moc: &Path, moc_args: &Option<String>, name: &str) -> Result<()> {
        match self.package_set.find(name) {
            None => Err(anyhow!(
                "The package \"{}\" does not exist in the package set",
                name
            )),
            Some(package) => {
                let mut cmd = Command::new(moc);
                cmd.arg("--check");
                if let Some(args) = moc_args {
                    cmd.args(args.split(' '));
                }
                download_package(package, false)?;
                let dependencies = self
                    .package_set
                    .transitive_deps(package.dependencies.clone());
                for package in dependencies {
                    let path = download_package(package, false)?;
                    cmd.arg("--package").arg(&package.name).arg(path);
                }

                package.sources().for_each(|entry_point| {
                    cmd.arg(entry_point);
                });
                let output = cmd.output().context(format!("Failed to run {:?}", cmd))?;
                if output.status.success() {
                    let warnings = String::from_utf8(output.stderr)?;
                    if !warnings.is_empty() {
                        info!("Verified \"{}\" with output:\n{}", package.name, warnings);
                    } else {
                        info!("Verified \"{}\"", package.name);
                    }
                    Ok(())
                } else {
                    Err(anyhow!(
                        "Failed to verify \"{}\" with:\n{}",
                        package.name,
                        String::from_utf8(output.stderr)?
                    ))
                }
            }
        }
    }

    pub fn verify_all(&self, moc: &Path, moc_args: &Option<String>) -> Result<()> {
        let mut errors: Vec<(Name, Error)> = vec![];
        for package in &self.package_set.topo_sorted() {
            if errors.iter().any(|(n, _)| package.dependencies.contains(n)) {
                if let Err(err) = self.verify_package(moc, moc_args, &package.name) {
                    errors.push((package.name.clone(), err))
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            let err = anyhow!(
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

/// Guards against path strings in package data
fn is_valid_dirname(input: &str) -> bool {
    input
        .chars()
        .all(|c| c.is_alphanumeric() || "-_.".contains(c))
        && !input.trim_matches('.').is_empty()
        && !input.starts_with('-')
}

/// Checks package name string
fn validate_name(name: &str) -> &str {
    assert!(is_valid_dirname(name), "Invalid package name: `{}`", name);
    name
}

/// Checks package or compiler version string
fn validate_version(version: &str) -> &str {
    assert!(
        is_valid_dirname(version),
        "Invalid version string: `{}`",
        version
    );
    version
}

pub fn download_compiler(version: &str) -> Result<PathBuf> {
    let bin = Path::new(".vessel").join(".bin");
    let dest = bin.join(validate_version(version));
    if dest.exists() {
        return Ok(dest);
    }

    let tmp = Path::new(".vessel").join(".tmp");
    if !tmp.exists() {
        fs::create_dir_all(&tmp)?
    }

    let os = if cfg!(target_os = "linux") {
        ("x86_64-linux", "linux64")
    } else if cfg!(target_os = "macos") {
        ("x86_64-darwin", "macos")
    } else {
        return Err(anyhow!(
            "Installing the compiler is only supported on Linux or MacOS for now"
        ));
    };
    let target = match Version::parse(version) {
        Ok(semver) if semver > Version::new(0, 6, 2) => format!(
            "https://github.com/dfinity/motoko/releases/download/{}/motoko-{}-{}.tar.gz",
            version, os.1, version
        ),
        _ => format!(
            "https://download.dfinity.systems/motoko/{}/{}/motoko-{}.tar.gz",
            version, os.0, version
        ),
    };

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&target)
        .header(reqwest::header::USER_AGENT, "vessel")
        .send()?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to download Motoko binaries for version {}, with \"{}\"\n\nDetails: {}",
            version,
            response.status(),
            response
                .text()
                .unwrap_or_else(|_| "No more details".to_string())
        ));
    }

    // We unpack into a temporary directory and rename it in one go once
    // the full unpacking was successful
    let tmp_dir: TempDir = tempfile::tempdir_in(tmp)?;
    Archive::new(GzDecoder::new(response)).unpack(tmp_dir.path())?;

    if !bin.exists() {
        fs::create_dir_all(&bin)?
    }
    fs::rename(tmp_dir, &dest)?;

    Ok(dest)
}

/// Downloads a package either as a tar-ball from Github or clones it as a repo
pub fn download_package(package: &Package, force: bool) -> Result<PathBuf> {
    let vessel_dir = Path::new(".vessel");
    // Always validate the name here
    let package_dir = vessel_dir.join(validate_name(&package.name));
    if !package_dir.exists() {
        fs::create_dir_all(&package_dir).context(format!(
            "Failed to create the package directory at {}",
            package_dir.display()
        ))?;
    }
    // Always validate the version here
    let repo_dir = package_dir.join(validate_version(&package.version));
    if force && repo_dir.exists() {
        fs::remove_dir_all(&repo_dir)?;
    }
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

    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to download tarball for repo \"{}\" at version \"{}\", with \"{}\"\n\nDetails: {}",
            repo,
            version,
            response.status(),
            response.text().unwrap_or_else(|_| "No more details".to_string())
        ));
    }

    // We unpack into a temporary directory and rename it in one go once
    // the full unpacking was successful
    let tmp_dir: TempDir = tempfile::tempdir_in(tmp)?;
    Archive::new(GzDecoder::new(response)).unpack(tmp_dir.path())?;

    // We expect an unpacked repo to contain exactly one directory
    let repo_dir = match fs::read_dir(tmp_dir.path())?.next() {
        None => return Err(anyhow!("Unpacked an empty tarball for {}", repo)),
        Some(dir) => dir?,
    };

    if !repo_dir.path().is_dir() {
        return Err(anyhow!("Failed to unpack tarball for \"{}\"", repo));
    }
    fs::rename(repo_dir.path(), dest)?;

    Ok(())
}

/// Clones `repo` into `dest` and checks out `version`
fn clone_package(tmp: &Path, dest: &Path, repo: &str, version: &str) -> Result<()> {
    let tmp_dir: TempDir = tempfile::tempdir_in(tmp)?;
    let clone_result = Command::new("git")
        .args(&["clone", repo, "repo"])
        .current_dir(tmp_dir.path())
        .output()
        .context(format!("Failed to clone the repo at {}", repo))?;
    if !clone_result.status.success() {
        return Err(anyhow!(
            "Failed to clone the repo at: {}\nwith:\n{}",
            repo,
            std::str::from_utf8(&clone_result.stderr).unwrap()
        ));
    }

    let repo_dir = tmp_dir.path().join("repo");
    let checkout_result = Command::new("git")
        .args(&["-c", "advice.detachedHead=false", "checkout", version])
        .current_dir(&repo_dir)
        .output()
        .context(format!(
            "Failed to checkout version {} for the repository {} in {}",
            version,
            repo,
            repo_dir.display()
        ))?;
    if !checkout_result.status.success() {
        return Err(anyhow!(
            "Failed to checkout version {} for the repo at: {}\nwith:\n{}",
            version,
            repo,
            std::str::from_utf8(&checkout_result.stderr).unwrap()
        ));
    }

    fs::rename(repo_dir, dest)?;
    Ok(())
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
}

type Hash = String;

/// Fetches the latest release of dfinity/vessel-package-set and computes its
/// Dhall hash. This way it can be used to initialize the package-set file.
pub fn fetch_latest_package_set() -> Result<(Url, Hash)> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get("https://api.github.com/repos/dfinity/vessel-package-set/releases")
        .header(reqwest::header::ACCEPT, "application/vnd.github.v3+json")
        .header(reqwest::header::USER_AGENT, "vessel")
        .send()?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to read Github releases: {:#?}",
            response
        ));
    }
    let releases: Vec<GhRelease> = response.json()?;
    let release = &releases[0].tag_name;
    fetch_package_set_impl(&client, release)
}

/// Like `fetch_latest_package_set`, but lets you specify the tag
pub fn fetch_package_set(tag: &str) -> Result<(Url, Hash)> {
    let client = reqwest::blocking::Client::new();
    fetch_package_set_impl(&client, tag)
}

fn fetch_package_set_impl(client: &reqwest::blocking::Client, tag: &str) -> Result<(Url, Hash)> {
    let package_set_url = format!(
        "https://github.com/dfinity/vessel-package-set/releases/download/{}/package-set.dhall",
        tag
    );
    let package_set = client
        .get(&package_set_url)
        .send()
        .context("When downloading the package set release")?
        .text()
        .context("When decoding the package set release")?;
    let hash = hash_dhall_expression(&package_set).context("When hashing the package set")?;
    Ok((package_set_url, hash))
}

/// Computes the sha256 hash for a given Dhall expression
/// Computes the sha256 hash for a given Dhall expression
fn hash_dhall_expression(expr: &str) -> Result<String> {
    let dhall_expr = dhall::syntax::text::parser::parse_expr(expr)
        .context(format!("Failed to parse a dhall expression: {}", expr))?;
    let hash = dhall_expr
        .sha256_hash()
        .context(format!("Failed to hash the expression: {:?}", dhall_expr))?;
    let formatted_hash = format!("{}", dhall::syntax::Hash::SHA256(hash));
    Ok(formatted_hash)
}

/// Initializes a new vessel project by creating a `vessel.dhall` file with no
/// dependencies and adding a small package set referencing vessel-package-set
pub fn init() -> Result<()> {
    let package_set_path: PathBuf = PathBuf::from("package-set.dhall");
    let manifest_path: PathBuf = PathBuf::from("vessel.dhall");
    let (package_set_url, hash) = match fetch_latest_package_set() {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to fetch latest package-set. Initializing with an older fallback version.\n\nDetails: {}", e);
            ("https://github.com/dfinity/vessel-package-set/releases/download/mo-0.4.3-20200916/package-set.dhall".to_string(),
             "sha256:3e1d8d20e35550bc711ae94f94da8b0091e3a3094f91874ff62686c070478dd7".to_string())
        }
    };
    if package_set_path.exists() {
        return Err(anyhow!(
            "Failed to initialize, there is an existing package-set.dhall file here"
        ));
    }
    if manifest_path.exists() {
        return Err(anyhow!(
            "Failed to initialize, there is an existing vessel.dhall file here"
        ));
    }
    let mut manifest = fs::File::create("vessel.dhall")?;
    manifest.write_all(
        br#"{
  dependencies = [ "base", "matchers" ],
  compiler = None Text
}
"#,
    )?;
    let mut manifest = fs::File::create("package-set.dhall")?;
    write!(&mut manifest, "let upstream = {} {}", package_set_url, hash)?;
    manifest.write_all(
        br#"
let Package =
    { name : Text, version : Text, repo : Text, dependencies : List Text }

let
  -- This is where you can add your own packages to the package-set
  additions =
    [] : List Package

let
  {- This is where you can override existing packages in the package-set

     For example, if you wanted to use version `v2.0.0` of the foo library:
     let overrides = [
         { name = "foo"
         , version = "v2.0.0"
         , repo = "https://github.com/bar/foo"
         , dependencies = [] : List Text
         }
     ]
  -}
  overrides =
    [] : List Package

in  upstream # additions # overrides
"#,
    )?;
    Ok(())
}

pub type Url = String;

pub type Tag = String;

pub type Name = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, serde_dhall::StaticType)]
pub struct Package {
    pub name: Name,
    pub repo: Url,
    pub version: Tag,
    pub dependencies: Vec<Name>,
}

impl Package {
    pub fn install_path(&self) -> PathBuf {
        Path::new(".vessel")
            .join(validate_name(&self.name))
            .join(validate_version(&self.version))
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

// This isn't normalized, as the package name is duplicated, but it's too handy
// to have a `Package` carry its name along.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageSet(pub HashMap<Name, Package>);

#[derive(Debug, PartialEq, Eq, Default, Serialize, Deserialize, serde_dhall::StaticType)]
pub struct Manifest {
    pub compiler: Option<String>,
    pub dependencies: Vec<Name>,
}

impl PackageSet {
    fn new(packages: Vec<Package>) -> PackageSet {
        let mut package_set = HashMap::new();
        for package in packages {
            package_set.insert(package.name.clone(), package);
        }
        PackageSet(package_set)
    }

    /// Finds a package by name
    fn find(&self, name: &str) -> Option<&Package> {
        self.0.get(name)
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
        for (name, package) in &self.0 {
            ts.insert(name.as_ref());
            for dep in &package.dependencies {
                ts.add_dependency(dep.as_ref(), name.as_ref())
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
        let ps = PackageSet::new(vec![a.clone(), b.clone()]);
        assert_eq!(vec![&b], ps.transitive_deps(vec!["B".to_string()]));
        assert_eq!(vec![&a, &b], ps.transitive_deps(vec!["A".to_string()]))
    }

    #[test]
    fn it_finds_transitive_dependencies_with_overlaps() {
        let a = mk_package("A", vec!["B"]);
        let b = mk_package("B", vec![]);
        let c = mk_package("C", vec!["B"]);
        let ps = PackageSet::new(vec![a.clone(), b.clone(), c.clone()]);
        assert_eq!(
            vec![&a, &b, &c],
            ps.transitive_deps(vec!["A".to_string(), "C".to_string()])
        );

        assert_eq!(vec![&b, &c], ps.transitive_deps(vec!["C".to_string()]))
    }

    #[test]
    fn it_validates_package_strings() {
        // Valid names/versions
        for input in ["a", "A", "a.b", "123", "1.2.3", ".0", ".a", "_"] {
            println!("{}", input);
            assert!(std::panic::catch_unwind(|| validate_name(input)).is_ok());
            assert!(std::panic::catch_unwind(|| validate_version(input)).is_ok());
        }

        // Invalid names/versions
        for input in [
            "", ".", "..", "...", "/", "\\", "a/b", "a\\b", "~", "-", "-a",
        ] {
            println!("{}", input);
            assert!(std::panic::catch_unwind(|| validate_name(input)).is_err());
            assert!(std::panic::catch_unwind(|| validate_version(input)).is_err());
        }
    }
}
