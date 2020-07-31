{
  system ? builtins.currentSystem,
}:

let nixpkgs = import ./nix { inherit system; }; in
let stdenv = nixpkgs.stdenv; in
let subpath = p: import ./nix/gitSource.nix p; in

let vessel = nixpkgs.rustPlatform.buildRustPackage rec {
  pname = "vessel";
  version = "1.0.0";
  buildInputs = [
    nixpkgs.openssl
    nixpkgs.openssl.dev
  ];
  nativeBuildInputs = [
    nixpkgs.pkg-config
  ];

  src = subpath ./.;

  cargoSha256 = "16q9i5cjkcd1gwy7ac12zvdjmfcw4na0fmillxpzkmjsp4iw25pb";
  verifyCargoDeps = true;
}; in

rec {
  inherit vessel;

  all-systems-go = nixpkgs.releaseTools.aggregate {
    name = "all-systems-go";
    constituents = [
      vessel
    ];
  };

  # include shell in default.nix so that the nix cache will have pre-built versions
  # of all the dependencies that are only depended on by nix-shell.
  shell =
    let extra-pkgs = [
          nixpkgs.easy-dhall.dhall-simple
          nixpkgs.easy-dhall.dhall-lsp-simple
    ]; in

    vessel.overrideAttrs (old: {
      nativeBuildInputs = (old.nativeBuildInputs or []) ++ extra-pkgs ;
    });
}
