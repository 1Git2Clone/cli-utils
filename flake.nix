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
      rustManifest = "Cargo.toml";

      # Both Rust crates build from the one workspace; each package selects its
      # crate with `-p`.
      rustSrc =
        lib:
        lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./tools/ls-posts
            ./tools/yt-audio
          ];
        };

      # One build per feature set: buildRustPackage copies buildFeatures into
      # cargoBuildFeatures/cargoCheckFeatures at evaluation, so overrideAttrs on
      # buildFeatures cannot change them.
      ytAudio =
        pkgs: buildFeatures:
        pkgs.rustPlatform.buildRustPackage {
          pname = "yt-audio";
          version = "0.1.2";
          src = rustSrc pkgs.lib;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [
            "-p"
            "yt-audio"
          ];
          cargoTestFlags = [
            "-p"
            "yt-audio"
          ];
          inherit buildFeatures;

          # Same reason as ls-posts above.
          preConfigure = ''
            export CARGO_HOME="$NIX_BUILD_TOP/cargo-home"
          '';

          nativeBuildInputs = [ pkgs.makeWrapper ];
          postInstall = ''
            wrapProgram $out/bin/yt-audio \
              --prefix PATH : ${
                pkgs.lib.makeBinPath [
                  pkgs.yt-dlp
                  pkgs.ffmpeg
                ]
              }
          '';
          meta = {
            description = "Download YouTube audio with tidy tags and filenames";
            license = pkgs.lib.licenses.mit;
            mainProgram = "yt-audio";
          };
        };

      # One attribute per tool. The language is an implementation detail of this
      # function; only the attribute name is the public interface.
      toolsFor =
        pkgs:
        import ./tools/scripts { inherit pkgs; }
        // {
          ls-posts = pkgs.rustPlatform.buildRustPackage {
            pname = "ls-posts";
            version = "0.1.0";
            src = rustSrc pkgs.lib;
            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [
              "-p"
              "ls-posts"
            ];
            cargoTestFlags = [
              "-p"
              "ls-posts"
            ];

            # nixpkgs' cargoSetupHook sets no CARGO_HOME, so cargo falls back to
            # its default of $HOME/.cargo -- and creates it, on any invocation,
            # `cargo metadata` included. Sandboxed that is harmless. UNSANDBOXED,
            # as on the Forgejo runner, $HOME is /homeless-shelter and Nix never
            # creates it, so cargo makes it for real on the job container's root;
            # Nix then refuses to start every LATER builder in that job with
            #
            #   error: home directory "/homeless-shelter" exists; please remove
            #   it to assure purity of builds without sandboxing
            #
            # naming the innocent build rather than this one. `.#default` is a
            # symlinkJoin over this package, i.e. a second builder in the same
            # command, so it tripped that every time.
            #
            # preConfigure rather than an `env` attribute: NIX_BUILD_TOP is only
            # known at build time, and it differs between a sandboxed build
            # (/build) and an unsandboxed one (/nix/var/nix/builds/nix-*). This
            # makes the build pure with respect to $HOME on BOTH forges, which is
            # what lets .forgejo/workflows/ci.yml be a plain twin of the GitHub
            # one instead of carrying a `rm -rf /homeless-shelter` dance.
            preConfigure = ''
              export CARGO_HOME="$NIX_BUILD_TOP/cargo-home"
            '';
            meta = {
              description = "Reconstruct gallery-dl post URLs from a folder tree";
              license = pkgs.lib.licenses.mit;
              mainProgram = "ls-posts";
            };
          };

          yt-audio = ytAudio pkgs [ "llm" ];
        };

      packagesFor =
        pkgs:
        (toolsFor pkgs)
        // {
          # Both variants install bin/yt-audio, so only the LLM-enabled one is
          # in toolsFor (and hence the `default` bundle); this one stands alone.
          yt-audio-no-llm = ytAudio pkgs [ ];

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

        tmux = import ./tools/scripts/tmux/tests.nix {
          pkgs = nixpkgs.legacyPackages.${system};
          tools = self.packages.${system};
        };
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
