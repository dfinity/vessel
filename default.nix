{ system ? builtins.currentSystem, nixpkgs ? import ./nix { inherit system; } }:
with nixpkgs;
let
  subpath = import ./nix/gitSource.nix { inherit system nixpkgs; } ;
  noNixFile = name: type:
    let baseName = builtins.baseNameOf (builtins.toString name);
    in !(lib.hasSuffix ".nix" name);
  vessel = rustPlatform.buildRustPackage rec {
    pname = "vessel";
    version = "0.6.5";
    buildInputs = [ 
      openssl_3_0 openssl_3_0.dev 
      ] ++ pkgs.lib.optional pkgs.stdenv.isDarwin
        pkgs.darwin.apple_sdk.frameworks.Security;
    nativeBuildInputs = [ pkg-config ];
    src = lib.sources.cleanSourceWith {
      filter = noNixFile;
      src = subpath ./.;
    };
    cargoSha256 = "sha256-Guy2pi8qWRJWendOadGD6GPZpjEbbmEQ2OqiixrNqK4=";
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
