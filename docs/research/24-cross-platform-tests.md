# 24 — How mature Rust CLIs support Linux, macOS and Windows, in code and in tests

Status: research, 2026-11-06. Background for D263's follow-up: "Windows builds and lints
now; its tests need a Windows test kit and the fixes found by running there". Builds on
D263 and D169; cited, not repeated.

Question: how do mature Rust CLI projects structure cross-platform support in production
code and, above all, in tests? What should loci adopt? The maintainer's principle is to
keep `#[cfg]` sites few and not scattered.

What loci has today (commit 9685c53 plus an unstaged doc-comment edit in
`crates/loci-testkit/src/lib.rs`):

- **Production `cfg` sites: three, in three files.** `vault::stage` sets mode 0o666
  under `#[cfg(unix)]` (`crates/loci-core/src/vault.rs` lines 366–367).
  `plugin::is_executable` has three `cfg` attributes (`crates/loci-core/src/plugin.rs`
  lines 87–92). `dispatch` names the killing signal under `#[cfg(unix)]` /
  `#[cfg(not(unix))]` (`crates/loci/src/commands/dispatch.rs` lines 39–43).
  `plugin::discover` strips `std::env::consts::EXE_SUFFIX` with no `cfg` (line 71).
- **Test kit.** `TestVault::empty` makes `bin/` the only `PATH` entry and symlinks
  `git` into it, with a `cfg` pair choosing `symlink` or `symlink_file`
  (`crates/loci-testkit/src/lib.rs` lines 50–55). `env` calls `env_clear` and then sets
  `PATH`, `HOME`, `GIT_CONFIG_NOSYSTEM` and the author and committer variables (lines
  104–115). `install_stub` and `Stub::script` are `#[cfg(unix)]`. They write a
  `#!/bin/sh` script named `loci-stub` with mode 0o755 (lines 215–221, 411–425).
  `redact` replaces the root string and the temp-dir string (lines 225–230). The root is
  `std::fs::canonicalize`d (line 56).
- **Integration tests: all `#![cfg(unix)]`.** These are `crates/loci/tests/{fix,
  lint_fixtures, m1_m4, mutate, plugins, workflow}.rs` and
  `crates/loci-learn/tests/learn.rs`. `properties.rs` has no gate. Of 134 tests, the
  ones that call a Unix API (`std::os::unix`, `PermissionsExt`, `OsStrExt`) are in
  `lint_fixtures` (13 uses), `m1_m4` (4), `mutate` (2) and `learn` (4). `fix`,
  `plugins` and `workflow` call none, so they are gated only because of the kit.
- **CI.** `nix flake check` runs on x86_64-linux, aarch64-linux and aarch64-darwin. The
  `windows-2025` job runs `cargo build` and `cargo clippy -D warnings` only
  (`.github/workflows/ci.yml`). The repository has no `.gitattributes`.

Conventions as in research 12 to 23: `[unverified]` means I did not confirm the claim
in a primary source, `[design inference]` is my own extrapolation, and `[gap]` means I
looked and found nothing. Sources are pinned to the commits listed under Sources. Line
numbers refer to those commits. I did not run anything on Windows.

## TL;DR

| Project | Where production `cfg` lives | Test platform layer | Gating style in tests | Path and line-ending normalization | Helper executables |
|---|---|---|---|---|---|
| cargo | small `cfg` pairs per helper in `cargo-util` / `cargo-test-support` | `cargo-test-support` (`symlink_supported()`, `SymlinkBuilder`, `FileBuilder::executable`) and the `#[cargo_test]` proc macro | mostly runtime: `#[cargo_test(ignore_windows = "…")]` emits `#[ignore]`; `if !symlink_supported() { return }`, which is **forced true on CI**; also 36 `#[cfg(unix)]` and 30 `#[cfg(windows)]` test items | snapbox `FilterPaths` (`\` to `/`) plus `FilterNewlines`, then `[ROOT]` redactions | tiny Cargo projects built at test time (`echo_wrapper`); executables written with `EXE_SUFFIX` |
| jj | **one `mod platform` per OS** in `local-util/src/file_util.rs`, re-exported under one name | `testutils` (hermetic git config) and `cli/tests/common` (`TestEnvironment`) | `check_symlink_support()?` early return; `#![cfg(unix)] // Nothing to test on Windows` for a whole file; `cfg!(unix)` / `cfg!(windows)` inside a test for the expected output | `dunce::canonicalize` of the env root; regex replaces the native and slash forms of the root and turns `\` to `/` after it; `jj.exe` to `jj` | **Rust `[[bin]]` fakes** (`fake-editor`, `fake-formatter --abort`, …) with `required-features = ["test-fakes"]` |
| gitoxide | `gix-path` holds the Windows git/sh discovery | `gix-testtools`: bash fixture scripts, `configure_git_environment`, `bash_program()` | runtime skip only off CI (`should_skip_as_git_version_is_smaller_than` returns false on CI) | `.gitattributes`: `*.sh text eol=lf` | shell fixture scripts run with Git for Windows' `bash.exe` |
| ripgrep | `cfg` pairs in tests' `util.rs` (`link_dir` / `link_file`) | `tests/util.rs` (`Dir`, `TestCommand`) | `#[cfg(not(windows))]` / `#[cfg(unix)]` on single tests (13 in all) | the tool itself: every test runs `rg --path-separator /` | `CARGO_BIN_EXE_rg` |
| uv | `uv-fs::Simplified` (`dunce`) | `uv-test` crate (`TestContext`) | `#[cfg(unix)]` / `#[cfg(windows)]` items and `cfg!` branches | `INSTA_FILTERS`: `\\([\w\d]\|\.)` to `/$1`, `uv\.exe` to `uv`; path regexes match both `\` and `/`; `.gitattributes` `* text=auto eol=lf` | a fake `git` shell script (Unix-only in effect) |

What they share. No project keeps every test portable, and none puts a `cfg` on every
Unix-flavoured line either. Each one has a single platform layer in its test-support
crate: symlinks, the executable bit, the executable suffix, the temp-root shape. Tests
call that layer instead of `std::os::*`. A feature that cannot exist on Windows is
gated at compile time (jj gates whole files). A capability that a machine may or may not
have (symlinks, a minimum git version) is checked at run time, and cargo and gitoxide
make that check **fail rather than skip on CI**. Snapshot output is normalised once, in
the kit. Two projects pin line endings with `.gitattributes`, and every project builds
its helper programs in Rust when they must run on Windows.

## 1. cargo (master 111443f)

**Platform layer.** `crates/cargo-test-support/src/lib.rs` keeps each OS difference in a
`cfg` pair next to the helper that needs it:

- `FileBuilder::mk` appends `env::consts::EXE_SUFFIX` to an executable's name, and only
  on Unix sets `mode | 0o111` (lines 150–168).
- `SymlinkBuilder::mk` is `#[cfg(unix)]` (`os::unix::fs::symlink`) and
  `#[cfg(windows)]` (`symlink_dir` or `symlink_file`, which also fixes the `.exe`
  extension) (lines 198–219).
- `symlink_supported()` (lines 1606–1636):

  ```rust
  #[cfg(windows)]
  pub fn symlink_supported() -> bool {
      if is_ci() {
          // We want to be absolutely sure this runs on CI.
          return true;
      }
      … os::windows::fs::symlink_file(&src, &dst) …
          eprintln!("symlinks not supported: {:?}\n\
                     Windows 10 users should enable developer mode.", e);
  ```

  On Unix it is `true`. Tests use it as a runtime skip, 28 times in
  `tests/testsuite`: `if !symlink_supported() { return; }` (for example
  `lockfile_path.rs` lines 57–61).
- `paths.rs` uses `cfg!(windows)` for short test-directory names: "Due to path-length
  limits, Windows doesn't use the full test name" (lines 68–73).

**The `#[cargo_test]` macro** (`crates/cargo-test-macro/src/lib.rs`) turns conditions
into `#[ignore = "reason"]`: `nightly`, `requires = "<cmd>"`, and
`ignore_windows = "reason"`, which is `set_ignore!(cfg!(windows), …)` (lines 62, 175–176).
A rule needs a reason (lines 181–185). `has_command` panics instead of skipping when
`CARGO_TEST_REQUIRE_EXTERNAL_TOOLS` is set (lines 393–419). Its `is_executable` is the
same as loci's: the execute bit on Unix and `is_file()` on Windows (lines 421–432).
`ignore_windows` is rare (5 uses). The reasons given: "PATH can't be overridden on
Windows", "setting file attributes is a bit complicated", "no method to prevent
creating or locking a file".

**Counts in `tests/testsuite`.** There are 36 `#[cfg(unix)]`, 30 `#[cfg(windows)]` and 14
`cfg!(windows)`. Gates sit on single tests, not files. `death.rs` keeps the one
OS-specific action in a helper pair: `#[cfg(unix)] fn ctrl_c` calls
`libc::kill(-pid, SIGINT)`, and `#[cfg(windows)] fn ctrl_c` calls `child.kill()`
(lines 91–102).

**Normalisation.** `compare.rs` documents that "Backslashes are converted to forward
slashes to deal with Windows paths" (lines 30–40). `normalize_actual` applies snapbox
`FilterPaths` and `FilterNewlines`, then the `[ROOT]` / `[EXE]` redactions (lines
456–463).

**Helper binaries.** cargo's tests make a helper program by writing a small Cargo
project and building it with cargo. An example is `echo_wrapper`
(`tests/testsuite/utils/tools.rs` lines 16–46). External-subcommand tests build
`cargo-foo` and rename it to `cargo-foo{EXE_SUFFIX}` (`cargo_command.rs` lines
420–445). Others write an empty file with `.executable(…)` for listing-only tests
(lines 95–110). Windows-only behaviour gets its own test: a `Path` variable instead of
`PATH` (lines 112–135). Cargo has cargo at hand, so this costs it little. loci does not.

**CI.** The Windows matrix entries run `cargo test -p cargo` (`.github/workflows/main.yml`
line 241, matrix lines 168–191). Since `symlink_supported()` is forced true there, the
symlink tests run on GitHub's Windows runners and pass.

## 2. jj (main 8cdeee2)

**One platform module in production.** `local-util/src/file_util.rs` re-exports
`platform::{check_symlink_support, symlink_dir, symlink_file}` (lines 34–38). It defines
`#[cfg(unix)] mod platform` (lines 374–459) and `#[cfg(windows)] mod platform` (lines
461–514) with the same function names. On Unix `check_symlink_support` is `Ok(true)`.
On Windows it reads the Developer Mode registry value
`AppModelUnlock\AllowDevelopmentWithoutDevLicense` (lines 483–489). The rest of jj
calls these names with no `cfg`. jj also has loci's 0o666 pattern, in the same shape:
`#[cfg(unix)]` `builder.permissions(from_mode(0o666))` with a `#[cfg(not(unix))]` plain
builder (lines 230–240).

**Test environment.** `cli/tests/common/test_environment.rs`:

- `env_clear()`, then **the host `PATH` is kept** "to allow accessing executables like
  `git`". `HOME` is set. On Unix `TMPDIR` is set. On Windows `USERPROFILE` and
  `APPDATA` point into the test home and `TEMP` is kept "stable" (lines 125–150).
- Git isolation: `GIT_CONFIG_SYSTEM` and `GIT_CONFIG_GLOBAL` point to
  `hermetic_git_config_path()`. That is `/dev/null` on Unix. On Windows it is a
  temp-dir path that cannot exist, because "Git for Windows 2.52 (ARM64) rejects [`NUL`]
  with 'unable to access 'NUL': Invalid argument'" (`lib/testutils/src/lib.rs` lines
  105–121).
- The env root is `dunce::canonicalize(env_dir.path())` (line 53).
- `normalize_output` replaces `jj.exe` with `jj`. For each registered path, a regex
  matches the native form and the rest of the path, and the rest has `\` turned into
  `/`. On Windows the slash-separated form is replaced as well (lines 300–321).

**Gating in tests.** In `lib/tests` there are 17 `#[cfg(unix)]`, one file-level
`#![cfg(unix)] // Nothing to test on Windows`
(`test_local_working_copy_executable_bit.rs` line 15) and 11 `cfg_attr(windows,
ignore = "…")` (flaky gpg tests). In `cli/tests` there are 33 `#[cfg(unix)]` and 18
`cfg!` branches. Symlink tests skip at run time, with a message:
`if !check_symlink_support()? { eprintln!("Skipping test because symlink isn't
supported"); return Ok(()); }` (`cli/tests/test_file_chmod_command.rs` lines 153–158).

**Fake programs are Rust `[[bin]]`s.** `cli/Cargo.toml` declares `fake-bisector`,
`fake-echo`, `fake-editor`, `fake-diff-editor` and `fake-formatter`, each with
`path = "testing/…rs"` and `required-features = ["test-fakes"]` (lines 31–58). The crate
enables the feature for its own tests through a dev-dependency on itself: `jj-cli = {
path = ".", features = ["test-fakes"], default-features = false }` (line 137; feature
line 143). Tests find them with `assert_cmd::cargo::cargo_bin!("fake-editor")`
(`cli/tests/common/mod.rs` lines 24–41).

**A crash test that runs on both systems.** `fake-formatter --abort` raises SIGTERM on
Unix and then calls `std::process::abort()` (`cli/testing/fake-formatter.rs` lines
240–245). The test branches on `cfg!(unix)` for "terminated by signal: 15 (SIGTERM)" and
on `cfg!(windows)` for `[exit status: -1073740791]`. The comment there reads "abort
produces STATUS_STACK_BUFFER_OVERRUN (0xc0000409)" (`cli/tests/test_util_command.rs`
lines 242–269).

**CI.** Windows x86_64 and aarch64 run `cargo nextest run --workspace` (`.github/workflows/
ci.yml` lines 40–45, 104). A composite action moves `TMP`/`TEMP` to `D:` and enables
8.3 short names (`.github/actions/setup-windows/action.yml`). jj's `.gitattributes` only
marks lock files (`linguist-generated`, `merge=binary`). Its tests write their files
from Rust literals at run time, and rustc normalises CRLF in source (see §7), so
checkout line endings do not reach jj's test data.

## 3. gitoxide (main f06f106)

**Fixtures are bash scripts.** `tests/tools/src/lib.rs` (gix-testtools) runs fixture
scripts with `bash_program()`. On Windows that searches `<git root>/bin/bash.exe` and
then `<git root>/usr/bin/bash.exe`, falling back to `bash.exe` on `PATH` (lines
2546–2575). The root comes from `git --exec-path`: `GIT_CORE_DIR` is
`<root>/<platform>/libexec/git-core` (lines 455–470), and `ancestors().nth(3)` gives the
root. The doc comment explains the order. "`bash.exe` in `System32`, which is
associated with WSL, would often be found first". The shim in `bin/` is preferred
because it "sets environment variables when run and is, on some systems, needed to find
the POSIX utilities that scripts need".

**`gix-path` does the same for `sh.exe`.** `BIN_DIR_FRAGMENTS = ["bin", "usr/bin"]`,
"ordered so that a shim is preferred over a non-shim" (`gix-path/src/env/auxiliary.rs`
lines 82–86, 97–145). It also declines to infer a root from a `libexec/git-core` that
does not sit inside a known MSYS2 prefix (`MSYS_USR_VARIANTS`).

**Environment.** `configure_git_environment` (lines 2449–2495) removes every inherited
`GIT_*` variable and `SSH_ASKPASS`, `BASH_ENV`, `ENV` and `CDPATH`. It sets
`GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=<null device>` (`nul` on Windows, line
2405), `GIT_ATTR_NOSYSTEM=1`, fixed dates and identities, and
`MSYS="… winsymlinks:nativestrict"`, so that MSYS `ln -s` makes real Windows symlinks
or fails. It does not clear the environment.

**Skips.** `should_skip_as_git_version_is_smaller_than` returns `false` on CI ("CI
should be made to use a recent git version, it should run there") (lines 509–514).
Archive generation is skipped on Windows, with the reason (lines 2578–2583). `umask()`
is `#[cfg(unix)]` (line 2849).

**`.gitattributes`.** "assure line feeds don't interfere with our working copy hash":
`*.sh text eol=lf`, `justfile text eol=lf`, and the SSH signature fixtures are
`text eol=lf`.

## 4. ripgrep (master 91f5d22)

- `tests/util.rs` holds the only platform pairs: `link_dir` / `link_file` as
  `#[cfg(not(windows))]` (`symlink`) and `#[cfg(windows)]` (`symlink_dir` /
  `symlink_file`) (lines 200–247). The binary is `CARGO_BIN_EXE_rg`, with a fallback to
  `../rg{EXE_SUFFIX}` (lines 180–184). `cmd_exists` runs `program --help` (lines 47–52).
- **The tool normalises its own output.** Every test command gets
  `--path-separator /` (line 171), so expected output never contains `\`.
- Gates are on single tests: 8 `#[cfg(not(windows))]`, 3 `#[cfg(unix)]`, 2
  `#[cfg(windows)]` and one `cfg!(windows)`. Some have reasons, for example "gives
  'access denied' errors when trying to create a file symlink" (`regression.rs` lines
  192–195). The non-UTF-8 name test `r210` is `#[cfg(unix)]`, and its comment reads
  "APFS does not support creating files with invalid UTF-8 bytes" (lines 268–276).
- No `.gitattributes`. The test data is inline in Rust sources.

## 5. uv (main a305592)

- `crates/uv-test/src/lib.rs` is uv's test kit. `INSTA_FILTERS` (lines 101–123) includes
  "Rewrite Windows output to Unix output", `(r"\\([\w\d]|\.)", "/$1")` and
  `(r"uv\.exe", "uv")`. `path_pattern` escapes a path and replaces each `\\` with
  `(\\|\/)`, so one filter matches either separator. Both the canonical and the given
  form are added (lines 1934–1962).
- `test_bucket_dir` canonicalises the temp dir with `simple_canonicalize`, that is
  `dunce::canonicalize`, "because on macOS (and Windows on GitHub Actions) the standard
  temp dir is a symlink" (lines 925–940; `crates/uv-fs/src/path.rs` lines 51–61).
- `disallow_git_cli` writes a `#!/bin/sh` script named `git{EXE_SUFFIX}`, and only Unix
  sets 0o755 (lines 1249–1265). On Windows this still blocks git, because a shell
  script named `git.exe` will not run at all `[design inference]`.
- `.gitattributes`: `* text=auto eol=lf`.

## 6. Crates

- **`dunce` 1.0.5** (https://gitlab.com/kornelski/dunce, `src/lib.rs`). `canonicalize`
  is `fs::canonicalize` on non-Windows. On Windows it strips `\\?\` when the path can
  be written in legacy form ("converts paths to legacy format whenever possible"). "On
  non-Windows platforms these functions leave paths unmodified, so it's safe to use
  them unconditionally". It has a `cfg` pair inside, so callers need none. jj and uv use
  it.
- **`which`** (harryfei/which-rs README) is a cross-platform `which`. Its handling of
  `PATHEXT` is `[unverified]` (not read). loci's `find_on_path` with `EXE_SUFFIX` is
  enough for `git`/`sh`, which are `.exe`.
- **`same-file`** (BurntSushi) gives file identity across platforms. jj uses
  `same_file::Handle` as its Windows `FileIdentity` (`file_util.rs` line 491). loci does
  not need it today.

## 7. Windows facts that matter for loci

**Symlinks.** The `std::os::windows::fs::symlink_file` docs say: "Windows treats symlink
creation as a privileged action … Users can try enabling Developer Mode, granting the
`SeCreateSymbolicLinkPrivilege` privilege, or running the process as an administrator"
(`library/std/src/os/windows/fs.rs` at 1.91.0, lines 627–633). std passes
`SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE`, so Developer Mode is enough
(`library/std/src/sys/fs/windows.rs` lines 1383–1405). **GitHub's Windows images enable
Developer Mode.** `Configure-DeveloperMode.ps1` sets `AllowDevelopmentWithoutDevLicense
= 1` (actions/runner-images `images/windows/scripts/build/Configure-DeveloperMode.ps1`).
Git is installed with `/o:EnableSymlinks=Enabled` (`Install-Git.ps1`), which sets
`core.symlinks` in the system config (build-extra `installer/install.iss` line 3316).
So symlink tests can run on CI, as cargo's do. On Windows, `FileType::is_symlink` is true
for any name-surrogate reparse point, which includes junctions
(`sys/fs/windows.rs` lines 1147–1154). loci's symlink refusal should therefore cover
junctions too `[design inference; not run]`.

**Line endings.** The runner installs Git for Windows with no `CRLFOption`
(`Install-Git.ps1`). The installer default is `CRLFAlways`, which writes
`core.autocrlf=true` into the system config (`install.iss` lines 2365, 2373–2380,
3256–3267). `actions/checkout` has no line-ending option. Its issue #135, "Windows -
Line endings & conversion", is still open. So without `.gitattributes`, a Windows CI
checkout of loci gets CRLF in every text file. Three consequences follow:

1. **`include_str!` does not normalise CRLF.** The Rust reference says: "The
   `include_str!` and `include_bytes!` macros do not apply these transformations", that
   is BOM removal and CRLF normalization (reference `src/input-format.md` lines
   52–59). loci compiles `share/loci/rules.md`, `updates.md` and the skills in with
   `include_str!` (`crates/loci/src/assets.rs` lines 18–27;
   `crates/loci-learn/src/commands.rs` line 21). A Windows build from such a checkout
   would ship CRLF rules and skills. Rust source literals are safe, because rustc
   normalises CRLF in source (same file, lines 35–39).
2. **Fixtures read at run time** (`tests/fixtures/**`, 133 files) would gain CR bytes,
   which shifts byte spans and changes hashes. With `text=auto`, a file "already in Git
   with CRLF endings" is left alone (`Documentation/gitattributes.adoc` lines 157–163).
   So `tests/fixtures/frontmatter-edge/concepts/a/crlf.md` keeps its CRLF either way.
3. **insta snapshots are safe.** insta 1.49.0's `normalize` does
   `out.replace("\r\n", "\n")` for file and inline snapshots (`insta/src/snapshot.rs`
   lines 753–766).

The test vaults themselves are not affected. `TestVault::env` sets
`GIT_CONFIG_NOSYSTEM=1`, so the runner's `core.autocrlf` and `core.symlinks` do not reach
the repositories the tests create. On Windows, git's defaults then apply `[unverified]`.

**Where `git.exe` and `sh.exe` live.** The runner installs with
`/o:PathOption=CmdTools` and then adds `C:\Program Files\Git\bin` to the machine `PATH`
(`Install-Git.ps1`). `CmdTools` puts `\cmd`, `\mingw64\bin` and `\usr\bin` on `PATH`,
"List \cmd before \bin so \cmd has higher priority" (`install.iss` lines 3412–3431). The
image readme lists `gitbash.exe` as `C:\Program Files\Git\bin\bash.exe`, next to
`C:\Windows\System32\bash.exe` for WSL (`Windows2025-Readme.md` lines 141–143). Git is
2.56.0 (line 67).

- `cmd\git.exe` is **git-wrapper**, built from `git-wrapper.c` (MINGW-packages
  `mingw-w64-git/PKGBUILD` lines 172–176, 640–644). It **resolves its own symlinks**.
  `find_exe_realpath` opens `GetModuleFileNameW`'s path and calls
  `GetFinalPathNameByHandleW`, stripping `\\?\` (`git-wrapper.c` lines 46–97). So a
  **symlink to `cmd\git.exe` works**, and so does one to the `bin\` shims, which
  gitoxide calls shims ("`<git-root>/bin`, where Git for Windows installs executable
  shims", `auxiliary.rs` lines 7–9). That the `bin\*.exe` files are built from the same
  wrapper source is `[unverified]`.
- The wrapper **prepends to `PATH`**: `<root>\cmd` for `cmd\git.exe`; for the full-path
  shims, `<root>\mingw64\bin;<root>\usr\bin;` and `$HOME\bin` (`setup_environment`,
  lines 124–203). A `sh.exe` reached through the `bin\sh.exe` shim therefore finds
  `grep` and `head` without loci's test linking them in.
- `usr\bin\sh.exe` and `mingw64\bin\git.exe` are the real binaries. They load DLLs from
  their own folder (`msys-2.0.dll` is in `usr\bin`, `install.iss` line 690). Whether a
  symlink to them finds those DLLs is `[unverified]`. The Windows loader's "application
  directory" for a symlinked image was not checked. **Linking the wrapper avoids the
  question.**

**Program lookup in `std::process::Command` on Windows.** For a bare name, std searches
(1) the child's `PATH`, if the command sets one, (2) the directory of the current
executable, (3–4) the system and Windows directories, then (5) the parent's `PATH`. It
appends `.exe` (`library/std/src/sys/process/windows.rs` at 1.91.0, lines 444–572). For
loci this has two effects. A test that sets `PATH` is still not airtight: `loci.exe`'s
own folder (`target\debug`) and `System32` are searched too. And loci's own
`Command::new("git")` / `Command::new("sh")` inherit the same rules.

**`env_clear` on Windows.** "`std::process::Command::env_clear` is unusable on Windows" is
an open issue (rust-lang/rust#114737). It says that "Windows requires some system envs",
names `SystemRoot`, and notes that Go and libuv add it back. `std::env::temp_dir` uses
`GetTempPath2W`, which tries `TMP`, `TEMP`, `USERPROFILE`, and then "the Windows
directory" (Microsoft Learn, `GetTempPath2W`). `loci bench` calls
`tempfile::Builder::tempdir()` (`crates/loci/src/commands/bench.rs` line 143). After
loci-testkit's `env_clear`, a Windows run would make its copies under `C:\Windows`, or
fail without admin rights `[design inference]`.

**`canonicalize` gives `\\?\`.** Pushing onto a verbatim path rebuilds it from components,
so `\\?\C:\v`.join("views/x.md") becomes `\\?\C:\v\views\x.md`. A plain `C:\v` keeps the
pushed `/` (`library/std/src/path.rs` lines 1262–1350). `Vault::open` and
`git::toplevel` both canonicalise (`vault.rs` line 69, `git.rs` line 394), so the
comparison holds. But every `root.display()` in a message, and `LOCI_VAULT` for
plugins, would carry `\\?\`. Whether Git for Windows accepts `git -C \\?\C:\…` is
`[unverified]`.

**Exit codes.** On Windows a process always has a code, and it is 32 bits:
`abort()` gives `0xC0000409` (jj, above). `dispatch` returns
`ExitCode::from(c as u8)` (`dispatch.rs` line 36), so a plugin exiting with 256 would make
loci exit 0, and `abort()` would give 9 `[design inference; not run]`. On Unix, `code()`
is 0–255, so this is Windows-only.

**Executable bit and modes.** Windows has no execute bit. cargo, jj and loci all treat
"is a file" as executable there (cargo macro lines 427–432). `PermissionsExt` 0o000 and
0o555 have no Windows equivalent. cargo's reason for skipping a test is "setting file
attributes is a bit complicated". Making a file unreadable on Windows needs ACLs or an
open handle with a deny share mode `[unverified]`.

**Names that are not UTF-8.** These are Unix-only as bytes (`OsStrExt::from_bytes`).
ripgrep says APFS refuses them. loci's tests create `\xff` names, and the aarch64-darwin
CI job runs them. Whether they pass there is `[unverified]`: I did not read the CI logs.
On Windows the analogue is an unpaired UTF-16 surrogate (`OsStringExt::from_wide`).
Whether Git for Windows can list such a name is `[unverified]`.

## Implications for loci

### (1) Production `cfg`: one module in loci-core

Recommend a `crates/loci-core/src/platform.rs` holding the three differences, each
function with one `#[cfg(unix)]` and one `#[cfg(not(unix))]` body, called with no `cfg`
elsewhere (jj's `file_util::platform`):

- `stage_builder()` or `default_permissions(&mut tempfile::Builder)`: 0o666 on Unix
  (the same code jj has).
- `is_executable(&Metadata) -> bool`: execute bit, or any file (cargo and jj, same
  semantics).
- `killed_by(&ExitStatus) -> Option<String>`: the signal on Unix, `None` elsewhere.
  `dispatch` calls it, so the third site leaves `crates/loci`.

`EXE_SUFFIX` needs no `cfg` and stays where it is (cargo, ripgrep, uv).

Two more production changes:

- **Exit code passthrough** (§7). Map a code outside 0–255 to a failure, or to a
  `plugin.failed` refusal, instead of truncating it. This is plain code with no `cfg`.
  Ask the maintainer which.
- **`dunce::canonicalize`** for `Vault::open`, `git::toplevel` and `init` (jj, uv). It
  is one dependency with no `cfg` at the call sites, and messages and `LOCI_VAULT` lose
  `\\?\`. The alternative is to keep std and let tests redact `\\?\`. That works, but
  users would see `\\?\C:\…`.

Trade-off: the module adds a file and a layer for three one-line differences. The
user's guidelines say to propose a new abstraction before building it, so this needs
approval. The alternative is three local `cfg` pairs, which is today's state.

### (2) Test kit: one platform section in loci-testkit

Everything that differs by OS goes in the kit, so test files call kit functions, never
`std::os::*` (cargo-test-support, ripgrep `util.rs`, jj `testutils`):

- `TestVault::symlink(target, link)` and `symlink_dir`: a `cfg` pair, and on failure a
  **panic** naming Developer Mode (cargo's message). **No runtime skip.** CI has
  Developer Mode, and cargo forces the check true on CI anyway. This follows "fail
  loudly". It costs a Windows developer without Developer Mode a failing run instead of
  a skip, so ask the maintainer.
- `make_executable(path)`: 0o755 on Unix, nothing on Windows (cargo `FileBuilder`).
  Learn's hook tests and the fsmonitor hook test use it.
- `link_tool(name)`: links a host program into `bin/` as `name{EXE_SUFFIX}`. On Windows
  it resolves `git` and `sh` to **Git for Windows' wrappers**: `<root>\cmd\git.exe` and
  `<root>\bin\sh.exe`, with the root taken from `git --exec-path` as gitoxide does.
  Never `usr\bin\sh.exe` or `System32\bash.exe`. The wrappers resolve their own
  symlinks (`git-wrapper.c`), so a symlink is enough.
- `env` after `env_clear`: on Windows also pass `SystemRoot` (rust#114737; Go and libuv
  add it back), and set `TEMP` and `TMP` to a folder in the test's temp dir. Setting
  `TMPDIR` on Unix too keeps `bench` copies inside the test (jj sets `TMPDIR` and
  `TEMP`). Setting `USERPROFILE` and `APPDATA` to `home/` stops any program falling back
  to the real profile (jj). `HOME` is already set, and the wrapper uses it before
  `USERPROFILE` (`git-wrapper.c` lines 138–167). Consider replacing
  `GIT_CONFIG_NOSYSTEM` with `GIT_CONFIG_SYSTEM`/`GIT_CONFIG_GLOBAL` pointing to a
  missing file, and adding `GIT_ATTR_NOSYSTEM=1` (gitoxide). Avoid `NUL`, which breaks
  on ARM64 Git for Windows according to jj.
- Root: `dunce::canonicalize`, to match whatever production does (jj line 53, uv).
- `redact`: replace the root and the temp dir in both native and `/` forms, then turn `\`
  into `/` in the rest of each matched path (jj's `normalize_output`; uv's `(\\|\/)`).
  Also map `loci.exe` / `loci-learn.exe` to the bare names (jj, uv). Vault-relative paths
  already come from git with `/`.
- The stub: see (4).

The kit then has about five small `cfg` pairs in one place, and the test files have
none for these features.

### (3) Expressing platform-specific tests

Three classes, by what the test needs:

| Class | How expressed | loci tests |
|---|---|---|
| **A. Portable** | no gate | all of `fix.rs`, `workflow.rs`, `properties.rs`; `plugins.rs` except the crash assertion; most of `mutate.rs`, `lint_fixtures.rs`, `m1_m4.rs` and `learn.rs` |
| **B. Portable through the kit** | no gate; kit helpers from (2) | symlink refusals (`a_vault_with_a_symlink_is_refused` minus its `\xff` link; the links in `init_respects_the_vault_boundary`, `new_respects_the_vault_boundary` and the `.loci/export` test in `m1_m4.rs`; learn's `the_log_is_not_written_through_a_symlink` and `the_config_is_not_read_through_a_symlink`); executable hooks (`a_noisy_fsmonitor_hook_opens`, learn's failed-commit hook); `bench` tests (`sh`, plus `grep` and `head`, which the shim provides on Windows) |
| **C. Cannot exist on Windows** | compile time, **one `#![cfg(unix)]` file per test crate**, for example `crates/loci/tests/unix.rs` and `crates/loci-learn/tests/unix.rs` (jj's `#![cfg(unix)] // Nothing to test on Windows`) | modes (`a_write_that_cannot_be_made_changes_nothing`, `a_rename_that_cannot_be_made_undoes_the_earlier_ones`, the 0o000 part of `an_unreadable_file_refuses_the_vault`, learn's `a_failed_append_names_the_file`); non-UTF-8 byte names (`a_non_utf8_name_anywhere_refuses_the_vault`, the `\xff` parts of `a_deleted_file_does_not_refuse_the_vault`, the symlink test and the unreadable test); Unix sockets (the socket parts of `bench_gives_the_copy_a_repository_of_its_own` and `a_tracked_file_replaced_by_a_folder_or_a_special_file`); the signal-kill assertion |

Where a test mixes a portable part and a class-C part, split it: the class-C part moves to
`unix.rs`. That gives two `cfg` sites in total (one per crate) instead of seven
file-level gates. Runtime skips are used only for "this machine lacks X". With the
choice in (2) to fail loudly, loci would have **no runtime skips**, as cargo and
gitoxide behave on CI.

The alternative to `unix.rs` is a `#[cfg(unix)] mod unix { … }` at the bottom of each
topic file. It keeps related tests together but gives four or five sites. Ask the
maintainer.

`cfg!` branches inside a test, for the expected output (jj's crash test), are only worth
it where the behaviour exists on both systems but differs. For loci that is the
"plugin crashed" case: on Windows it is just an exit code, which the existing exit-code
test already covers. So the signal check goes to class C, not to a `cfg!` branch.

If the macOS `\xff` tests turn out to fail on APFS (§7), they need `target_os = "linux"`
or a runtime probe like jj's `check_strict_utf8_fs` (`lib/testutils/src/lib.rs` lines
1064–1077). Check the darwin CI log first.

### (4) The stub plugin

A `#!/bin/sh` file cannot be `loci-stub.exe`, and discovery on Windows requires the
`.exe` name (D263). Every project that runs helpers on Windows builds them in Rust
(jj's fakes, cargo's `echo_wrapper`). Options:

1. **A `[[bin]] loci-stub` in the `loci` package** with
   `required-features = ["test-stub"]`, enabled by a dev-dependency of `loci` on itself
   (jj `cli/Cargo.toml` lines 31–58, 137). Tests pass
   `env!("CARGO_BIN_EXE_loci-stub")` to `install_stub`. Cargo builds a bin before an
   integration test of the same package, "unless the binary has required features that
   are not enabled" (cargo `environment-variables.md` lines 289–295). A bin in
   `loci-testkit` would not get `CARGO_BIN_EXE_*` in `loci`'s tests, because the
   variable covers the package's own targets `[design inference from the doc's
   wording]`.
2. A `loci-testkit` build script that compiles the stub with `$RUSTC` into `OUT_DIR` and
   exposes the path through `env!`. This avoids the self-dependency and the feature, but
   adds a build script `[design inference; no studied project does this]`.

With either option: `install_stub` **copies** the binary to
`bin/loci-stub{EXE_SUFFIX}`, with no symlink privilege and no doubt about
`current_exe`. It writes the replies next to it (`bin/loci-stub.json`, read from
`current_exe().parent()`). The stub keeps today's behaviour: `lint` and `skills` replies
with exit codes, the stderr line, and "print args and `LOCI_VAULT`/`LOCI_VERSION`, exit
7". For `crash` it calls `std::process::abort()`, which is SIGABRT on Unix and
`0xC0000409` on Windows (jj). The signal assertion stays in class C. Option 1 touches
`crates/loci/Cargo.toml` and the nix package build flags, if they list bins
`[unverified]`. That needs the maintainer.

### (5) Windows specifics to handle

- **`.gitattributes`**: `* text=auto eol=lf` (uv), so `include_str!` assets and fixtures
  are LF on every checkout. Adding `tests/fixtures/** -text` makes fixtures byte-exact
  even for a future intentional CRLF fixture, which `text=auto` would otherwise
  normalise on commit (gitattributes doc lines 157–163). Without this, the existing
  Windows build job already embeds CRLF assets.
- **`\\?\`**: `dunce` in production and in the kit, or redact it.
- **Separators**: vault paths come from git (`/`). Absolute paths in messages are
  native. `redact` normalises them as in (2). Whether CLI arguments written with `\`
  (`loci mv views\a.md …`) should be accepted is a product question for the maintainer.
- **git and sh discovery**: production uses `Command::new("git")` and
  `Command::new("sh")`, with std's search order (§7). Tests link the Git for Windows
  wrappers. `bench` on Windows needs `sh` on `PATH`, which `CmdTools` installs provide.
  Document it in the RUNBOOK and README.
- **Env after `env_clear`**: `PATH`, `HOME`, `SystemRoot`, `TEMP`, `TMP`, plus
  `USERPROFILE` and `APPDATA` pointed into the test (jj). `PATHEXT` and `ComSpec` are
  probably not needed, since std appends `.exe` itself `[unverified for MSYS sh's own
  lookups]`.
- **Exit codes above 255**: fix in `dispatch` (1).
- Path length: test roots are short temp dirs, so cargo's `t{id}` trick is not needed
  yet `[design inference]`.

### (6) CI

- Windows job: add `cargo test --locked --workspace` after clippy (cargo and jj run their
  full suites on `windows-latest` / `windows-2022`). Optionally use `cargo nextest` (jj)
  for clearer failure reports. That would be a new tool, so only if wanted.
- Do not set `core.autocrlf` in the workflow. `.gitattributes` fixes it for every clone,
  CI or not (gitoxide, uv).
- Optionally use jj's `D:` temp-drive action for speed. It is not needed for
  correctness.
- Consider `windows-11-arm` later. cargo, jj and ripgrep test on it, and jj found the
  `NUL` git-config bug there.

### Questions for the maintainer

1. A `loci-core::platform` module for the three production differences, or keep three
   local `cfg` pairs?
2. Add `dunce` to production (users never see `\\?\`), or only redact it in tests?
3. Symlink tests on a Windows machine without Developer Mode: fail (recommended,
   matches CI) or skip with a message (jj)?
4. Class-C tests: one `tests/unix.rs` per crate, or a `mod unix` per topic file?
5. The stub: a `[[bin]]` behind a `test-stub` feature in `loci` (jj), or a testkit build
   script?
6. Plugin exit codes above 255: map to 1, or refuse with `plugin.failed`?
7. Should `bench` keep `sh -c` on Windows (D169, D263), given that it needs Git for
   Windows' `sh` on `PATH`?

## Gaps

- Not run on Windows. Every Windows behaviour above comes from source or docs.
- Symlinks to the real `usr\bin\sh.exe` / `mingw64\bin\git.exe` (DLL lookup):
  `[unverified]`.
- Git for Windows with `git -C \\?\C:\…`: `[unverified]`.
- The default `core.symlinks` in repositories created under `GIT_CONFIG_NOSYSTEM`, and
  whether `git ls-files` lists a std-created symlink there as a link: `[unverified]`.
- Whether loci's `\xff` tests pass on aarch64-darwin: `[unverified]`.
- `which`'s `PATHEXT` handling: not read.

## Sources

cargo master 111443f3dc4d82a65e2306a93028f122406401c1:
- https://github.com/rust-lang/cargo/blob/111443f3dc4d82a65e2306a93028f122406401c1/crates/cargo-test-support/src/lib.rs (lines 150–168, 198–219, 1606–1636)
- …/crates/cargo-test-support/src/paths.rs (lines 60–83, 420–476)
- …/crates/cargo-test-support/src/compare.rs (lines 25–40, 456–473)
- …/crates/cargo-test-macro/src/lib.rs (lines 25–62, 131–185, 393–455)
- …/tests/testsuite/death.rs (lines 91–102); …/tests/testsuite/cargo_command.rs (lines 95–135, 420–445); …/tests/testsuite/utils/tools.rs (lines 16–46); …/tests/testsuite/lockfile_path.rs (lines 57–61)
- …/.github/workflows/main.yml (lines 168–191, 241)
- …/doc/book/src/reference/environment-variables.md (lines 289–295); …/doc/book/src/reference/cargo-targets.md (lines 135–139)

jj main 8cdeee2c2c904458b0f562d6d2adaecf7f280d96:
- https://github.com/jj-vcs/jj/blob/8cdeee2c2c904458b0f562d6d2adaecf7f280d96/local-util/src/file_util.rs (lines 34–38, 230–240, 374–514)
- …/cli/tests/common/test_environment.rs (lines 53, 125–165, 300–321); …/cli/tests/common/mod.rs (lines 24–41)
- …/lib/testutils/src/lib.rs (lines 105–155, 1064–1077)
- …/cli/Cargo.toml (lines 31–58, 137, 143); …/cli/testing/fake-formatter.rs (lines 240–250)
- …/cli/tests/test_util_command.rs (lines 242–269); …/cli/tests/test_file_chmod_command.rs (lines 153–158); …/lib/tests/test_local_working_copy_executable_bit.rs (line 15); …/lib/tests/test_gpg.rs (line 232)
- …/.github/workflows/ci.yml (lines 22, 40–45, 79–80, 104); …/.github/actions/setup-windows/action.yml; …/.gitattributes

gitoxide main f06f106b1d5b158f90a41d1d64970be76c1cfd2b:
- https://github.com/GitoxideLabs/gitoxide/blob/f06f106b1d5b158f90a41d1d64970be76c1cfd2b/tests/tools/src/lib.rs (lines 455–470, 509–514, 2405–2408, 2410–2495, 2530–2583, 2849–2858)
- …/gix-path/src/env/auxiliary.rs (lines 7–145); …/.gitattributes

ripgrep master 91f5d22ef48bde459d90a8c7dbeab416e97d0f2b:
- https://github.com/BurntSushi/ripgrep/blob/91f5d22ef48bde459d90a8c7dbeab416e97d0f2b/tests/util.rs (lines 47–52, 170–247)
- …/tests/regression.rs (lines 192–195, 268–276, 1686–1693)

uv main a3055929c0c762ab75b95a6a2b9be349c651f5f3:
- https://github.com/astral-sh/uv/blob/a3055929c0c762ab75b95a6a2b9be349c651f5f3/crates/uv-test/src/lib.rs (lines 101–123, 925–940, 1249–1265, 1934–1962)
- …/crates/uv-fs/src/path.rs (lines 22–61); …/.gitattributes

Rust 1.91.0 std:
- https://github.com/rust-lang/rust/blob/1.91.0/library/std/src/os/windows/fs.rs (lines 607–640)
- …/library/std/src/sys/fs/windows.rs (lines 1147–1170, 1382–1405)
- …/library/std/src/sys/process/windows.rs (lines 260–282, 444–572)
- …/library/std/src/path.rs (lines 1262–1350); …/library/std/src/env.rs (lines 660–690)
- rust-lang/rust#114737 (open): https://github.com/rust-lang/rust/issues/114737
- Rust reference a2ae66d: https://github.com/rust-lang/reference/blob/a2ae66d7037cbedac68746b9510b24cc34b96d69/src/input-format.md (lines 35–59)

GitHub runner images (actions/runner-images fad015e):
- https://github.com/actions/runner-images/blob/fad015e47059f40cf0786dcb4593384f6a0720d3/images/windows/scripts/build/Install-Git.ps1
- …/images/windows/scripts/build/Configure-DeveloperMode.ps1
- …/images/windows/Windows2025-Readme.md (lines 17, 67, 141–143)
- actions/checkout issue #135 (open): https://github.com/actions/checkout/issues/135

Git for Windows:
- build-extra d12e2ff: https://github.com/git-for-windows/build-extra/blob/d12e2ff0e8f50d6fb78bdae76efd73f254b9ebd1/installer/install.iss (lines 690, 1358–1372, 2365–2380, 2784–2815, 3256–3267, 3316, 3412–3431)
- MINGW-packages 20a412e: https://github.com/git-for-windows/MINGW-packages/blob/20a412e3b61c853fb3a290f02e78f59611d20342/mingw-w64-git/git-wrapper.c (lines 46–97, 124–203); …/mingw-w64-git/PKGBUILD (lines 172–176, 626–649)
- git `Documentation/gitattributes.adoc` (master, fetched 2026-11-06; lines 157–199)

Crates and docs:
- insta 1.49.0: https://github.com/mitsuhiko/insta/blob/1.49.0/insta/src/snapshot.rs (lines 645–652, 753–766)
- dunce 1.0.5: https://gitlab.com/kornelski/dunce/-/blob/master/src/lib.rs (lines 1–102; master, unpinned)
- which: https://github.com/harryfei/which-rs (README); same-file: https://github.com/BurntSushi/same-file (README)
- Microsoft Learn, GetTempPath2W: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-gettemppath2w (fetched 2026-11-06)

loci (9685c53 plus the unstaged kit edit): `crates/loci-core/src/vault.rs` (lines 69,
366–367), `crates/loci-core/src/plugin.rs` (lines 71, 84–95, 115–119),
`crates/loci-core/src/git.rs` (line 394), `crates/loci/src/commands/dispatch.rs` (lines
33–51), `crates/loci/src/commands/bench.rs` (lines 83–91, 143),
`crates/loci/src/assets.rs` (lines 18–27), `crates/loci-learn/src/commands.rs` (line 21),
`crates/loci-testkit/src/lib.rs` (lines 50–56, 104–115, 215–230, 411–425),
`crates/loci/tests/*.rs`, `crates/loci-learn/tests/learn.rs`,
`.github/workflows/ci.yml`, `flake.nix`, `docs/DECISIONS.md` D169, D263.
