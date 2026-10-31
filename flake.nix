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
      });

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
      });

      homeManagerModules.default = import ./nix/hm-module.nix;
    };
}
