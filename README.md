# Vessel

The original package manager for the Motoko programming language.

## Getting started

1. Download a copy of the `vessel` binary [from the release page](https://github.com/dfinity/vessel/releases) or build one yourself
   1. For Ubuntu in `$HOME/bin` RUN `wget https://github.com/dfinity/vessel/releases/download/v0.7.1/vessel-linux64`

      For macOS in `/usr/local/bin` RUN: `wget https://github.com/dfinity/vessel/releases/download/v0.7.1/vessel-macos`
   2. Rename vessel-linux64 to vessel eg: RUN `mv vessel-linux64 vessel`
   3. Change permissions, `chmod +x vessel`
2. Run `vessel init` in your project root.
3. Edit `vessel.dhall` to include your dependencies (potentially also edit
   `package-set.dhall` to include additional package sources)
4. In a dfx project: Edit `dfx.json` under defaults->build->packtool to say `"vessel sources"` like so:
   ```
   ...
   "defaults": {
     "build": {
       "packtool": "vessel sources"
     }
   }
   ...
   ```
   Then run `dfx build`
4. In a non-dfx project: Run `$(vessel bin)/moc $(vessel sources)
   -wasi-system-api main.mo` to compile the `main.mo` file with the installed
   packages in scope and using the `wasi` API to let you run the generated WASM
   with tools like [wasmtime](https://wasmtime.dev).

## How it works

Vessel is inspired by the [spago](https://github.com/purescript/spago) package
manager for PureScript. Any git repository with a `src/` directory is a valid
package to Vessel, which is a flexible and lightweight approach to package
management, that is easily extended with more guarantees and features as our
community grows. The two concepts you need to understand to work with Vessel
are _package sets_ and the _manifest_ file.

### Package sets

Vessel uses the idea of a _package set_ to manage where it pulls dependencies
from. A package set is a collection of packages at certain versions that are
known to compile together. The package set also specifies the dependencies
between these packages, so that Vessel can find all the transitively needed
packages to build your project. There will be a community maintained package set of
publicly available, open source packages. You can then base your projects
package set on the public one and extend it with your private and local
packages. The package set your project uses is stored in the `package-set.dhall`
file by default.

### Manifest file

Your `vessel.dhall` file contains the list of packages you need for your project
to build. Vessel will look at this file, and figure out all the transitive
packages you need using the package set file. Optionally it also contains a
compiler version that Vessel uses to download the compiler binaries for you.
Any change to this file requires a reload of the language service so your
packages can be picked up by your editor for now.

After Vessel has installed all required packages through cloning or
downloading tarballs, it puts them in a project local location (the `.vessel`
directory).

## How Tos

### How do I reset all caches?

Remove the `.vessel` directory in your project

### How do I depend on a git branch of a package?

The `"version"` field in the package set format refers to any git ref so you can
put a branch name, a commit hash or a tag in there.

**CAREFUL:** Vessel has no way of invalidating "moving" references like a
branch name. If you push a new commit to the branch you'll need to run `vessel install --force` to bypass your local cache.

### How do I add a local package to my package set?

Make sure your local package is a git repository, then add an entry like so to
your `additions` in the `package-set.dhall` file:

```dhall
let additions = [
   { name = "mypackage"
   , repo = "file:///home/path/to/mypackage"
   , version = "v1.0.0"
   , dependencies = ["base"]
   }
]
```

Now you can depend on this package by adding `mypackage` to your `vessel.dhall` file.

### How do I integrate Vessel into my custom build?

Running `vessel sources` will return flags in a format you can pass directly to
the various compiler tools. Running `vessel bin` returns the path containing the
compiler binaries. Use like so: `$(vessel bin)/mo-doc`.

## License
Vessel is distributed under the terms of the Apache License (Version 2.0).

See LICENSE for details.
