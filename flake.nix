{
  description = "loci: a linter and toolkit for Markdown knowledge vaults";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      # x86_64-pc-windows-gnu builds and unit tests under wine (nix/windows.nix).
      windows = nixpkgs.lib.genAttrs [ "x86_64-linux" ] (
        system:
        import ./nix/windows.nix {
          pkgs = nixpkgs.legacyPackages.${system};
          inherit (self.packages.${system}) loci;
        }
      );
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          p = import ./nix/package.nix { inherit pkgs; };
        in
        p // { default = p.loci; }
      );

      checks = forAllSystems (pkgs: {
        inherit (self.packages.${pkgs.system}) loci loci-learn;
        # The package's source and vendored dependencies, with Clippy for the build.
        clippy = self.packages.${pkgs.system}.loci.overrideAttrs (old: {
          pname = "loci-clippy";
          nativeBuildInputs = old.nativeBuildInputs ++ [ pkgs.clippy ];
          buildPhase = "cargo clippy --offline --workspace --all-targets -- -D warnings";
          doCheck = false;
          installPhase = "touch $out";
          dontFixup = true;
        });
        fmt =
          let
            src = pkgs.lib.fileset.toSource {
              root = ./.;
              fileset = pkgs.lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./rustfmt.toml ./crates ];
            };
          in
          pkgs.runCommand "loci-fmt" { nativeBuildInputs = [ pkgs.cargo pkgs.rustfmt ]; } ''
            cd ${src}
            cargo fmt --check
            touch $out
          '';
      } // nixpkgs.lib.optionalAttrs (windows ? ${pkgs.system}) { windows = windows.${pkgs.system}.check; });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
            cargo-insta
          ];
        };
      } // nixpkgs.lib.optionalAttrs (windows ? ${pkgs.system}) { windows = windows.${pkgs.system}.shell; });
    };
}
