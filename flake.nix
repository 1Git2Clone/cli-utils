{
  description = "cli-utils - a multi-language collection of command-line utilities";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    git-hooks.url = "github:cachix/git-hooks.nix";
    git-hooks.inputs.nixpkgs.follows = "nixpkgs";
    systems.url = "github:nix-systems/default";
  };

  outputs =
    {
      self,
      nixpkgs,
      git-hooks,
      systems,
      ...
    }:
    let
      forAll = nixpkgs.lib.genAttrs (import systems);

      # Shared by the clippy, cargo-check and rustfmt hooks -- all three default
      # to settings.rust.cargoManifestPath. One line per Rust crate.
      rustManifest = "tools/ls-posts/Cargo.toml";

      # One attribute per tool. The language is an implementation detail of this
      # function; only the attribute name is the public interface.
      toolsFor = pkgs: {
        ls-posts = pkgs.rustPlatform.buildRustPackage {
          pname = "ls-posts";
          version = "0.1.0";
          src = ./tools/ls-posts;
          cargoLock.lockFile = ./tools/ls-posts/Cargo.lock;
          meta = {
            description = "Reconstruct gallery-dl post URLs from a folder tree";
            license = pkgs.lib.licenses.mit;
            mainProgram = "ls-posts";
          };
        };
      };

      packagesFor =
        pkgs:
        (toolsFor pkgs)
        // {
          # One installable bundle, so a consumer needs a single home.packages
          # entry rather than one per tool.
          default = pkgs.symlinkJoin {
            name = "cli-utils";
            paths = builtins.attrValues (toolsFor pkgs);
            meta.description = "Every cli-utils tool";
          };
        };
    in
    {
      packages = forAll (system: packagesFor nixpkgs.legacyPackages.${system});

      checks = forAll (system: {
        pre-commit-check = git-hooks.lib.${system}.run {
          src = ./.;
          settings.rust.cargoManifestPath = rustManifest;
          hooks = {
            nixfmt-rfc-style.enable = true;
            statix.enable = true;
            deadnix.enable = true;

            markdownlint = {
              enable = true;
              args = [
                "-c"
                ".markdownlint.json"
              ];
            };

            rustfmt.enable = true;
            clippy.enable = true;
            cargo-check.enable = true;
          };
        };

        package = self.packages.${system}.default;
      });

      devShells = forAll (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          pre-commit-check = self.checks.${system}.pre-commit-check;
        in
        {
          default = pkgs.mkShell {
            buildInputs =
              pre-commit-check.enabledPackages
              ++ (with pkgs; [
                cargo
                rustc
                clippy
                rustfmt
                rust-analyzer
              ]);
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            inherit (pre-commit-check) shellHook;
          };

          # What CI enters. buildInputs comes from enabledPackages rather than a
          # hand-written list, so a hook enabled above is automatically on PATH
          # here and the two cannot drift.
          ci = pkgs.mkShell {
            buildInputs = pre-commit-check.enabledPackages ++ [
              pkgs.pre-commit
              pkgs.git
              pkgs.cargo
              pkgs.rustc
            ];
            inherit (pre-commit-check) shellHook;
          };
        }
      );
    };
}
