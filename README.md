# vessel

A simple package manager for the Motoko programming language.

## Getting started
1. Download a copy of the `vessel` binary from the release page or build one yourself
2. Run `vessel init` in your project root
3. Run `vessel build src/main.mo` (adjust the path to point to your entry point)
4. Now `wasmtime main.wasm` should run your compiled project

## How it works
`vessel` is inspired by the [spago](https://github.com/purescript/spago) package manager for PureScript. Any git repository with a `src/` directory is a valid package to `vessel`, which is a flexible and lightweight approach to package management, that is easily extended with more guarantees and features as our community grows. The two concepts you need to understand to work with `vessel` are _package sets_ and the _manifest_ file.

### Package sets
`vessel` uses the idea of a _package set_ to manage where it pulls dependencies from. A package set is a set of packages at certain versions that are known to compile together. The package set also specifies the dependencies between these packages so `vessel` can find all the transitively needed packages to build your project. There will be a community maintained package set of publicly available, open source packages. You can then base your projects package set on the public one and extend it with your private and local packages. The package set your project uses is stored in the `package-set.json` file by default.

### Manifest file
Your `vessel.json` file contains the list of packages you need for your project to build. `vessel` will look at this file, and figure out all the transitive packages you need using the package set file. Any change to this file requires a reload of the language service so your packages can be picked up by your editor for now.

After `vessel` has installed all required packages through cloning or downloading tarballs, it puts them in a project local location (the `.vessel` directory) and passes the appropriate flags to the Motoko compiler.

## How Tos

### How do I reset all caches?
Remove the `.vessel` directory in your project

### How do I depend on a git branch of a package?
The `"version"` field in the package set format refers to any git ref so you can put a branch name, a commit hash or a tag in there.

__CAREFUL:__ `vessel` has no way of invalidating "moving" references like a branch name. If you push a new commit to the branch you'll need to manually reset your caches and re-install.

### How do I add a local package to my package set?
Make sure your local package is a git repository, then add an entry like so to your package set:
```json
{
    "name": "mypackage",
    "repo": "file:///home/path/to/mypackage",
    "version": "v1.0.0",
    "dependencies": ["stdlib"]
}
```
Now you can depend on this package by adding `mypackage` to your `vessel.json` file.

### How do I integrate `vessel` into my custom build?
Running `vessel build --list-packages` will return flags in a format you can pass directly to the various compiler tools.

## Licence

Copyright 2020 Christoph Hegemann

This software is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
