{
  description = "Boring File Browser (bfb): A TUI File Browser";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      nixpkgs,
      flake-utils,
      fenix,
      crane,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        toolchain = fenix.packages.${system}.stable.toolchain;
        devToolchain = fenix.packages.${system}.combine [
          fenix.packages.${system}.stable.toolchain
          fenix.packages.${system}.stable.rust-src
          fenix.packages.${system}.stable.rust-analyzer
        ];
        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

        # programs bfb shells out to at runtime
        runtimeDeps = [ pkgs.xdg-utils ];

        commonArgs = {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          src = craneLib.cleanCargoSource (craneLib.path ./.);
          strictDeps = true;
          nativeBuildInputs = [
            pkgs.mold
            pkgs.makeWrapper
          ];
          RUSTFLAGS = "-C link-arg=-fuse-ld=mold -C link-arg=-B${pkgs.mold}/bin";
        };
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        bfb = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            postInstall = ''
              wrapProgram $out/bin/bfb \
                --suffix PATH : ${pkgs.lib.makeBinPath runtimeDeps}
            '';
            meta = {
              description = "Boring File Browser: a simple TUI file browser";
              mainProgram = "bfb";
              license = pkgs.lib.licenses.mit;
            };
          }
        );
      in
      {
        packages.bfb = bfb;
        packages.default = bfb;
        apps.default = {
          type = "app";
          program = pkgs.lib.getExe bfb;
        };
        checks = {
          inherit bfb;
          clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );
          fmt = craneLib.cargoFmt { inherit (commonArgs) pname version src; };
          test = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });
        };
        devShells.default = pkgs.mkShell {
          packages = [
            devToolchain
            pkgs.mold
          ]
          ++ runtimeDeps;
        };
      }
    );
}
