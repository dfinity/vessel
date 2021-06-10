{ system ? builtins.currentSystem, nixpkgs ? import ./nix { inherit system; } }:
with nixpkgs;
let
  subpath = import ./nix/gitSource.nix;
  noNixFile = name: type:
    let baseName = builtins.baseNameOf (builtins.toString name);
    in !(lib.hasSuffix ".nix" name);
  vessel = rustPlatform.buildRustPackage rec {
    pname = "vessel";
    version = "1.0.0";
    buildInputs = [ openssl openssl.dev ];
    nativeBuildInputs = [ pkg-config ];
    src = lib.sources.cleanSourceWith {
      filter = noNixFile;
      src = subpath ./.;
    };
    cargoSha256 = "1sbcr3sp8126qzgnqd1v4jm09y11d4dgvsbwysv2xakrxp1vh3zq";
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
