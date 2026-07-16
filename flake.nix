{
  description = "truss - Rust project scaffolder with template sync and local registries";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      crane,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        craneLib = crane.mkLib pkgs;

        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;
          strictDeps = true;
          buildInputs = [ ];
          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.openssl
          ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        truss = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            pname = "truss";
            cargoExtraArgs = "-p truss-cli";
            doCheck = true;
            cargoTestExtraArgs = "--workspace";
          }
        );
      in
      {
        packages.default = truss;
        packages.truss = truss;

        apps.default = flake-utils.lib.mkApp {
          drv = truss;
        };

        checks = {
          inherit truss;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.rustup
            pkgs.mise
            pkgs.pkg-config
            pkgs.openssl
            pkgs.gitleaks
            pkgs.ripsecrets
          ];

          shellHook = ''
            eval "$(mise activate bash)"
            git config core.hooksPath .githooks 2>/dev/null || true
          '';
        };
      }
    );
}
