# Builds, lints and tests for x86_64-pc-windows-gnu from x86_64-linux, with no
# rustup: nixpkgs' MinGW rustc (its sysroot holds the Windows std), the MinGW
# linker, and wine to run the tests. Only unit tests run here: the integration
# tests need Git for Windows (loci-testkit's `link_tool`), so they run in the
# Windows CI job.
{ pkgs, loci }:
let
  cross = pkgs.pkgsCross.mingwW64;
  rustc = cross.buildPackages.rustc;
  target = "x86_64-pc-windows-gnu";
  env = {
    RUSTC = "${rustc}/bin/rustc";
    # Clippy's driver finds the Windows std here.
    SYSROOT = "${rustc.unwrapped}";
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${cross.buildPackages.gcc}/bin/x86_64-w64-mingw32-gcc";
    # Rust's MinGW std links `libpthread.a`.
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-L ${cross.windows.pthreads}/lib";
    # wine: nixpkgs' emulator for Windows (manual, "test cross-compilation using emulation").
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER = cross.stdenv.hostPlatform.emulator pkgs;
    WINEDEBUG = "-all";
  };
  tools = [
    pkgs.clippy
    # `x86_64-w64-mingw32-dlltool`, which rustc runs for `windows-sys`.
    cross.buildPackages.binutils
  ];
  # Unit tests wine cannot run. `--exact`: a renamed test is no longer skipped,
  # so it fails here rather than silently dropping out.
  unsupported = [
    # They run `git`, which wine has none of.
    "git::tests::status_against_git"
    "git::tests::status_ignores_submodules"
    "git::tests::toplevel_keeps_trailing_spaces"
    # In the build sandbox, wine cannot make a link ("Operation not supported").
    "vault::tests::copy_dir_names_a_link_it_cannot_copy"
  ];
  skips = pkgs.lib.concatMapStringsSep " " (t: "--skip ${t}") unsupported;
in
{
  # `nix develop .#windows`, then `cargo clippy --target ${target}` or
  # `cargo test --target ${target} --lib --bins`.
  shell = pkgs.mkShell (
    env
    // {
      packages = [ pkgs.cargo ] ++ tools;
      shellHook = ''export WINEPREFIX="''${XDG_CACHE_HOME:-$HOME/.cache}/loci-wine"'';
    }
  );

  check = loci.overrideAttrs (
    old:
    env
    // {
      pname = "loci-windows";
      nativeBuildInputs = old.nativeBuildInputs ++ tools;
      buildPhase = ''
        export HOME=$TMPDIR WINEPREFIX=$TMPDIR/wine
        cargo clippy --offline --target ${target} --workspace --all-targets -- -D warnings
        cargo test --offline --target ${target} --workspace --lib --bins --no-fail-fast -- --exact ${skips}
      '';
      doCheck = false;
      installPhase = "touch $out";
      dontFixup = true;
    }
  );
}
