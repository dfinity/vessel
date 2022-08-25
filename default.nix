{ system ? builtins.currentSystem, nixpkgs ? import ./nix { inherit system; } }:
with nixpkgs;
let
  subpath = import ./nix/gitSource.nix;
  noNixFile = name: type:
    let baseName = builtins.baseNameOf (builtins.toString name);
    in !(lib.hasSuffix ".nix" name);
  vessel = rustPlatform.buildRustPackage rec {
    pname = "vessel";
    version = "0.6.5";
    buildInputs = [ openssl_3_0 openssl_3_0.dev ];
    nativeBuildInputs = [ pkg-config ];
    src = lib.sources.cleanSourceWith {
      filter = noNixFile;
      src = subpath ./.;
    };
    cargoSha256 = "sha256-9nc4CU7Fh2XB4i2oPwh4SQqSu676VKOQv3M+EjvW9t8";
    verifyCargoDeps = true;
  };
in rec {
  inherit vessel;
  # include shell in default.nix so that the nix cache will have pre-built versions
  # of all the dependencies that are only depended on by nix-shell.
  shell =
    let extra-pkgs = [ easy-dhall.dhall-simple easy-dhall.dhall-lsp-simple ];
    in vessel.overrideAttrs (old: {
      nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ extra-pkgs;
    });
}
