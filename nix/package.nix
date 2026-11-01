{ pkgs }:
let
  inherit (pkgs) lib;

  lockedVersion =
    name:
    let
      matches = builtins.filter (p: p.name == name) (builtins.fromTOML (builtins.readFile ../Cargo.lock)).package;
    in
    assert lib.assertMsg (builtins.length matches == 1) "expected exactly one ${name} in Cargo.lock";
    (builtins.head matches).version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
      ../LICENSE
      (lib.fileset.maybeMissing ../share)
      # Fixture vaults read by the integration tests.
      ../tests
    ];
  };

  build =
    {
      pname,
      # The package's own skills, relative to the repo root.
      skills,
    }:
    pkgs.rustPlatform.buildRustPackage {
      inherit pname src;
      version = lockedVersion pname;
      cargoLock.lockFile = ../Cargo.lock;
      cargoBuildFlags = [ "-p" pname ];
      cargoTestFlags = [ "-p" pname ];
      # The integration tests build temporary git vaults.
      nativeCheckInputs = [ pkgs.git ];
      postInstall = ''
        mkdir -p $out/share/loci/skills $out/share/doc/${pname}
        cp -r ${skills}/. $out/share/loci/skills/
        cp LICENSE $out/share/doc/${pname}/
      '';
      meta.mainProgram = pname;
    };
in
{
  loci = build {
    pname = "loci";
    skills = "share/loci/skills";
  };
  loci-learn = build {
    pname = "loci-learn";
    skills = "crates/loci-learn/skills";
  };
}
