# 23 — How tools treat gitignored files and names that are not UTF-8

Status: research, 2026-11-04. Background for the user's direction "whenever the vault
is opened, ensure everything is UTF-8". Builds on D133, D152, D183 and D207 and on m0
§3 and §9.6; cited, not repeated.

Question: how do other tools deal with (1) gitignored files and (2) files whose names
are not valid UTF-8? The answer should settle what "everything" covers: the listed
files only, the names `git status` reports, or ignored files too. It should also settle
how a bad name is reported.

What loci has today (commit 81ae7f8 plus the staged changes, 2026-11-04):

- **The vault is what `git ls-files` lists.** `Repo::ls_files` runs `git ls-files -z
  --cached --others --exclude-standard`. That lists tracked files and the untracked
  files that no ignore rule matches. A name that is not UTF-8, or that holds a control
  character, goes to `LsFiles::bad_names` (`crates/loci-core/src/git.rs` lines 162–183,
  302–313, 331–334).
- **One bad listed name refuses the vault.** `walk` puts `bad_names` with the
  unreadable files, and `refuse_unsafe` refuses with `file.unreadable`
  (`crates/loci-core/src/vault.rs` lines 218–253, 257–271; `Vault::open` calls `walk`
  at line 85). This holds for every listed name, `plugins/` included, whether or not
  the file is still on disk (D183, m0 §3 line 131). The name is shown through
  `String::from_utf8_lossy`, so `b\xFF.md` prints as `` `b�.md` has a name that is not
  UTF-8 ``.
- **`git status` records skip names that are not UTF-8.** `utf8_records` drops a
  NUL-separated record that is not UTF-8 (`git.rs` lines 351–357). It is used for the
  `git status --porcelain=v2 -z` records (`parse_status`, line 363) and for the `git
  ls-tree -r -z HEAD` names in `Repo::status` (line 226). m0 §9.6 item 2 says such a
  path is not counted (D133). `Repo::status` feeds `loci status`, `lint --fix`'s
  dirty-tree check (`crates/loci/src/commands/lint.rs` line 99), `mv`, and lint's
  `sources.changed` and `material-dropped` (`crates/loci-core/src/lint/disk.rs` line
  43).
- **Ignored files are never listed, so never checked at open.** The one check that
  reaches ignored paths is on writes. `Vault::refuse_symlink` refuses a symlink
  anywhere on the way to a path loci writes, "even an ignored one" (`vault.rs` lines
  139–160; m0 §9.7 line 796). `new` and `mv` refuse an ignored destination
  (`path.ignored`, through `Repo::is_ignored`, `git.rs` line 155).

While this note was being written, an unstaged change in the working tree made
`utf8_records` return an error for a record that is not UTF-8 ("skipping it would hide
a change"). That is option A below. The citations above are to the staged version.

Conventions as in research 12 to 22: `[unverified]` means I did not confirm the claim
in a primary source, `[design inference]` is my own extrapolation, and `[gap]` means I
looked and found nothing. "Run" means I ran the program. The scratch repositories were
under `/tmp/claude-1000/…/scratchpad/research23/`, with `commit.gpgsign false`. Each
test used a file `b\xFF.md` (byte 0xFF is never valid UTF-8), an ignored folder `ign/`
holding `i\xFE.md`, and an ignored `z\xFD.log` (`.gitignore`: `ign/`, `*.log`).

## TL;DR

| Tool | How it finds files | Ignored files | A name that is not UTF-8 | How it shows the name | Source |
|---|---|---|---|---|---|
| git 2.56.0 | index plus a work-tree walk | not shown unless `--ignored`; an ignored folder is one entry `ign/` | paths are bytes; listed like any other | C-quoted octal (`"b\377.md"`) by default; raw bytes with `-z` | `config/core.adoc` 152–165; `git-status.adoc` 423–431; `i18n.adoc` 1–22; run |
| jj 0.46.0 | own walk of the working copy | ignored folders not entered; ignored files in walked folders are seen | work tree: skipped, **with a warning** (since 0.44.0). A git tree with such a name: **hard error**, the repo stops working | Rust `Debug`: `"b\xFF.md"` | `local_working_copy.rs` 1710–1729, 1759; `git_backend.rs` 1180–1183; `cli_util.rs` 3266–3287; CHANGELOG 430–433; run |
| ripgrep 15.2.0 (`ignore`, `globset`) | own walk; reads `.gitignore` itself, only inside a git repository by default | not entered | searched and printed like any other; globs match the raw bytes on Unix | raw bytes on Unix; JSON `{"bytes": base64}` | `GUIDE.md` 174–190; `globset/src/lib.rs` 617–618; `printer/src/jsont.rs` 160–201; run |
| cargo 0.102.0 (`cargo package`) | gix walk: tracked plus untracked-not-ignored | not listed, so never checked | **refused** for every file it would package | `Path::display`, lossy (`b�.md`) | `sources/path.rs` 702–714; `ops/cargo_package/mod.rs` 497–511, 1093–1103; run |
| cargo dirty check | `gix` status | dirty only if packaged | kept as a path, not skipped (`gix::path::from_bstr`) | n/a | `ops/cargo_package/vcs.rs` 192–245, 279–336; run |
| zola 0.23.6 | `walkdir` over `content/` | `ignored_content` globs, checked **after** the name is converted | **panics** (`to_str().unwrap()`), even for an ignored file | panic message only | `components/site/src/lib.rs` 194–253; run |
| Obsidian | the vault folder; no git | no `.gitignore`; "Excluded files" only hide files in some views | not documented `[gap]` | n/a | help pages `[unverified]` beyond quotes |
| Rust std / camino 1.2.6 | n/a | n/a | `Path::to_str` gives `None`; `to_string_lossy` and `display` replace bytes with U+FFFD; camino checks UTF-8 **once** at the edge | `{:?}` gives `"b\xFF.md"` | std docs; camino `README.md` 15–42, 66–78; run |

What they share: no tool checks the names of ignored files, except zola, and there it
is an accident of ordering, not a rule. Tools that need UTF-8 names refuse them
at the boundary (cargo, jj for committed trees, camino), or skip them with a warning
when they cannot track the file anyway (jj's working copy). No tool silently drops a
name from a change count. The tools that show a bad name either escape the bytes (git,
jj) or print them raw (ripgrep). Only cargo prints the lossy form.

## 1. git 2.56.0

**Paths are bytes.** "Git at the core level treats path names simply as sequences of
non-NUL bytes, there are no path name encoding conversions (except on Mac and
Windows)" (`Documentation/i18n.adoc` lines 14–17). The same page says path names "are
encoded in UTF-8 normalization form C" (lines 7–12). That is a convention, and git
does not enforce it. It adds that "many Git-based tools simply assume path names to
be UTF-8 and will fail to display other encodings correctly" (lines 21–22).

**Quoting.** By default, `core.quotePath` puts a path with "unusual" characters in
double quotes and writes bytes above 0x80 as octal escapes. With
`core.quotePath=false`, bytes above 0x80 are not unusual any more, and control
characters and `"` and `\` are still escaped. "Many commands can output pathnames
completely verbatim using the `-z` option" (`Documentation/config/core.adoc` lines
152–165). `git-status.adoc` lines 423–431 and `git-ls-files.adoc` lines 253–256 say
the same for those commands. Run:

| Command | Output for `b\xFF.md` |
|---|---|
| `git ls-files --cached --others --exclude-standard` | `"b\377.md"` |
| `git ls-files -z …` | `b\377.md\0` (raw byte 0xFF) |
| `git status --porcelain=v2` | `? "b\377.md"` |
| `git -c core.quotePath=false status --porcelain=v1` | `?? b` + raw 0xFF + `.md` |

**Ignored files.** `git status` lists untracked files that are "not ignored by
gitignore" (`git-status.adoc` line 20). Ignored files are "not listed unless
`--ignored` is used", and then they are marked `!` (lines 202–203, 246). In the
default and `matching` modes, an ignored folder is one entry and its contents are
not shown (lines 111–128). Run with `--ignored`: `! ign/` and `! "z\375.log"`. The
name inside `ign/` never appears. `git ls-files -o -i --exclude-standard --directory`
gives the same two entries. Without `--directory` it lists every file inside ignored
folders, which in a real vault includes `.loci/` and anything a user keeps there.

**Where a name can come from.** A `git status` record names a path in the index,
in `HEAD`, or untracked in the work tree. With `--untracked-files=all` and without
`--ignored`, loci's `ls-files` call already lists the index and the untracked part.
The only names `status` can show that `ls-files` does not list are paths in `HEAD`
but not in the index. That covers a staged deletion (`git rm`) and a file removed with
`git rm --cached` that an ignore rule then matches `[design inference]`, checked for
the first case by a run:

```
git rm b\xFF.md      # committed before
git ls-files -z --cached --others --exclude-standard   → a.md only
git status --porcelain=v2 -z                            → 1 D. N... … b\377.md
git ls-tree -r --name-only HEAD                         → a.md, "b\377.md"
```

## 2. jj (Jujutsu) 0.46.0

**Paths are `String`s.** `RepoPathComponentBuf` and `RepoPathBuf` each wrap a single
`value: String` (`core/src/repo_path.rs` lines 40–44, 206–212).

**Working copy: skip and warn.** `process_dir_entry`
(`lib/src/local_working_copy.rs` lines 1710–1729) converts the entry's file name
with `into_string()`. If that fails, it sends `(dir, name)` to `invalid_utf8_paths` and
skips the entry: "A path that isn't valid UTF-8 can't be represented as a RepoPath, so
it can never be tracked. Skip it instead of failing the whole snapshot, and let the
caller report it." `SnapshotStats::invalid_utf8_paths` carries them
(`lib/src/working_copy.rs` lines 236–241), and `print_invalid_utf8_paths`
(`cli/src/cli_util.rs` lines 3266–3287) prints a warning. Before 0.44.0, snapshotting
failed instead (CHANGELOG lines 430–433, issue #9774). Run:

```
$ jj st
Warning: Skipped some paths because they are not valid UTF-8:
  .: "b\xFF.md"
Working copy changes:
A .gitignore
A a.md
```

**Ignored files.** The name check comes before the ignore check
(`git_ignore.matches_dir`, line 1759; `matches_file`, line 1789). An ignored folder is
not entered, so `ign/i\xFE.md` was not reported. An ignored file in a folder jj does
walk *was* reported. With `*.log` ignored, `jj st` warned about `"z\xFD.log"` (run).

**Committed trees: hard error.** Reading a git tree entry runs
`str::from_utf8(entry.filename())` and maps a failure to `BackendError::InvalidUtf8`
(`lib/src/git_backend.rs` lines 1027–1028, 1180–1183). Run: in a git repository whose
commit has `b\xFF.md`, `jj git init --colocate` and every later command (`jj st`, `jj
log`, `jj file list`) fail with "Internal error: Failed to reset the working copy …
Invalid UTF-8 for object 7337737… of type tree", exit 255.

So jj draws its line at the point where a name has to become a `RepoPath`. A
work-tree name it would not track anyway is a warning. A name in history, which it
must represent, stops everything.

## 3. ripgrep 15.2.0 and the `ignore` crate

**Ignored files.** ripgrep reads ignore files itself. It does not ask git. It skips
`.gitignore` matches, including global and repository excludes, "Unless the
`--no-require-git` flag is given" only inside a git repository (`GUIDE.md` lines
174–190; `WalkBuilder::git_ignore` and `require_git`, `crates/ignore/src/walk.rs`
lines 875, 900). An ignored folder is not entered. `rg --files` did not list `ign/` or
`z\xFD.log`, and `-uu` listed both (run).

**Names that are not UTF-8.** These are handled as bytes. `Gitignore::matched_stripped`
builds a `globset::Candidate` from the path (`crates/ignore/src/gitignore.rs` lines
259–270). `Candidate::new` calls `Vec::from_path_lossy` (`crates/globset/src/lib.rs`
lines 617–618). Despite the name, that call is lossless on Unix: bstr's
`from_os_str_lossy` returns `os_str.as_bytes()` under `cfg(unix)` and decodes lossily
only elsewhere (bstr 1.13.1 `src/ext_vec.rs` lines 283–325; ripgrep's lock has
1.13.0). Run: `rg -n b` printed `./b` + raw 0xFF + `.md:1:b` (checked with `od`).

**Machine output.** In JSON, a path is `{"text": …}` when it is UTF-8 and `{"bytes":
<base64>}` otherwise (`crates/printer/src/jsont.rs` lines 160–201). On Windows it falls
back to a lossy string. The code says "it's not clear what we should actually do"
(lines 193–200). Run: `{"type":"begin","data":{"path":{"bytes":"Yv8ubWQ="}}}`.

fd uses the same `ignore` walker. I did not run it or read its printing code
`[unverified]`.

## 4. cargo (`cargo package`), 0.102.0

Source: rust-lang/cargo master at 9511315 (Cargo.toml version 0.102.0). Run with cargo
1.101.0-nightly (31d3246e0, 2026-10-27).

**What it lists.** Without `package.include`, `list_files_gix` walks with
`emit_tracked(true)`, `emit_untracked(Matching)` and `emit_ignored(None)`
(`src/sources/path.rs` lines 702–714). That is git's tracked plus
untracked-not-ignored set, the same as loci's `ls-files`.

**Names that are not UTF-8 are refused, but only among listed files.**
`build_ar_list` calls `check_filename` on each listed file, then `to_str()` ("non-utf8
path in source directory") (`src/ops/cargo_package/mod.rs` lines 497–511).
`check_filename` bails with "path does not have a unicode filename which may not
unpack on all platforms" (lines 1093–1103). Run:

| Situation | `cargo package --list --allow-dirty` |
|---|---|
| `ign/i\xFE.md` ignored | lists the package, no error |
| plus an untracked `b\xFF.md` | `error: path does not have a unicode filename … b�.md`, exit 101 |

The name in the error is lossy (`Path::display`).

**Dirty check.** `collect_statuses` turns each status location into a path with
`gix::path::from_bstr`, which keeps the bytes on Unix, and pushes it to
`dirty_files`. Nothing is skipped by encoding (`src/ops/cargo_package/vcs.rs` lines
279–336). Only dirty files that would be packaged are reported (lines 222–245). Run: a
staged deletion of a committed `b\xFF.md` did not stop `cargo package --no-verify`,
because a deleted file is not packaged. This is the same gap loci has, but it comes
from scope, not from encoding.

## 5. zola 0.23.6 (markdown site generator)

`Site::load` walks `content/` with `walkdir`, drops walk errors (`Some(Err(_)) =>
continue`), and runs `name.to_str().unwrap()` on every entry *before* the
`ignored_content` globset is checked (`components/site/src/lib.rs` lines 194–227 at
master 48d4e7a). The per-section index walk does the same (line 253). Run with nixpkgs
zola 0.23.6, `ignored_content = ["*.log"]`, and `content/z\xFD.log`: `zola check`
panicked at `components/site/src/lib.rs:253:83`, "called `Option::unwrap()` on a
`None` value". This is the one tool that rejects an ignored file's name, and it does
so with a panic, not a design. It is a counter-example, not a precedent.

mdBook: not checked `[gap]`.

## 6. Obsidian (docs only)

- No git integration in core, so there is no gitignore. "Excluded files will be hidden
  in Search, Graph View, and Unlinked Mentions … and less noticeable in Quick Switcher
  and link suggestions" (https://obsidian.md/help/settings). The page does not say
  whether excluded files are still indexed. Forum posts say they are `[unverified]`.
- File names: "Obsidian will respect the filename limitations of the operating system
  you create the note on", and users should "make sure your filenames are safe for
  other operating systems" (https://obsidian.md/help/Files+and+folders/Manage+notes).
  There is nothing on names that are not UTF-8 `[gap]`.

## 7. Rust std and camino 1.2.6

- `Path::to_str`: "Yields a `&str` slice if the `Path` is valid unicode … non-UTF-8
  strings are perfectly valid for some OS". `to_string_lossy`: "Any non-UTF-8 sequences
  are replaced with U+FFFD REPLACEMENT CHARACTER". `display` "may perform lossy
  conversion … If you would like an implementation which escapes the path please use
  `Debug` instead" (https://doc.rust-lang.org/std/path/struct.Path.html, fetched
  2026-11-04).
- Run (rustc 1.101.0-nightly): for the bytes `b\xFF.md`, `{:?}` of the `OsStr` gives
  `"b\xFF.md"`, `display` and `from_utf8_lossy` give `b�.md`, and `to_str` gives `None`.
  The lossy form cannot be typed back to name the file, and two bad names can print
  the same. The `Debug` form can be typed in a shell as `$'b\xFF.md'`.
- camino's rationale (`README.md` lines 15–42): non-Unicode paths are "vanishingly
  uncommon". Unicode is "the common subset of supported paths across Windows and
  Unix". "There are already many systems, such as Cargo, that only support UTF-8
  paths". The "makefile problem", a file that names other files, "has no general,
  cross-platform solution in systems that support non-UTF-8 paths". So camino lets you
  "check that your paths are UTF-8 once, and then manipulate them as valid UTF-8 from
  there on". It names "Your system has files that contain the names of other files"
  as a reason to use it (lines 72–73). A vault of links is such a system. D183's
  `VaultPath` is the same design.

## 8. loci today, run

With `target/debug/loci` built from the working tree, in a vault made with `loci init`:

| Situation | Result |
|---|---|
| untracked `inbox/b\xFF.md` | refused: `file.unreadable`, `` `inbox/b�.md` has a name that is not UTF-8 `` |
| that file committed, then `git rm` (staged deletion) | `loci status`: `0 modified, 0 deleted, 0 added, 0 untracked`; `lint --fix` ran ("applied 0 fix(es)") on a tree git calls dirty |
| ignored `ign/i\xFE.md` | opens; nothing reported |
| ignored symlink `ign2 -> /tmp` (in `.git/info/exclude`) | opens; nothing reported (writes through it are refused by `refuse_symlink`) |

The staged-deletion row is the only hole found. `status` names a change that
`ls-files` does not, `utf8_records` drops it, and two commands then report a clean
tree.

## Implications for loci

"Everything" can be pinned down from §1. The names any loci command can meet are
the listed ones (`ls-files`: index plus untracked-not-ignored) and the names in
`HEAD` (`ls-tree`, and the `HEAD` side of `git status`). Ignored names are never
read, listed or written by loci.

**A. Refuse a name that is not UTF-8 in `git status` and `ls-tree HEAD` output, where
it is read** (the working-tree change in progress). `Repo::status` returns an error,
`loci status` shows it as `status.git`, and `lint --fix`, `mv` and lint's disk rules
refuse or warn as they do for any git failure. Precedent: jj refuses a committed
tree with such a name (§2). cargo keeps such names in its dirty list instead of
dropping them (§4). No tool silently drops a name from a change count. The cost: the
check runs per command, not at open, so `loci status` still opens the vault and
reports the problem as a warning, not a refusal. m0 §9.6's "is not counted" and D133
need to change.

**A′. Also check `HEAD` names at vault open.** `walk` runs `git ls-tree -r -z
--name-only HEAD` too (when there is a `HEAD`) and adds any name that is not UTF-8 to
`file.unreadable`. With §1, listed names plus `HEAD` names cover every name `git
status` can report without `--ignored`. "Every vault path is UTF-8" then holds at open,
which is the user's direction, and A's error in `Repo::status` becomes a guard that
should never fire. This is camino's "check once" applied to the whole git view, and
jj's rule that a name in history must be representable. The cost: one more git call
per open, linear in the size of `HEAD` [design inference: not measured]. And a vault
whose `HEAD` holds such a name stays refused until the fix is committed, not just
staged. The message must say "commit the rename". `[design inference]`

**B. Do not check ignored files.** No surveyed tool checks names inside ignored folders.
git does not list them even with `--ignored`. jj and ripgrep do not enter them. cargo
never lists them. Only zola touches ignored names, and it panics. loci never reads,
lists or names an ignored file (m0 §3). The write-side risk, an ignored symlink on the
way to a written path, is already refused per write (`refuse_symlink`). Checking ignored
names would mean `git ls-files -o -i --exclude-standard` without `--directory`, which
lists everything in `.loci/` and any large ignored folder. It would refuse vaults for
files loci will never touch.

**B′. Check ignored files.** The only precedent is jj's warning for an ignored *file*
in a folder it walks, which is a side effect of checking names before ignore rules.
Nothing supports a refusal. Not recommended.

**Reporting.** Keep one `file.unreadable` refusal that names each bad name. Show the
name escaped, like D207's `{:?}` for control characters, so `b\xFF.md` prints as
`"b\xFF.md"`, not `b�.md`. Precedent: jj prints the `Debug` form, and git C-quotes
with octal by default. The lossy form (used by cargo and by loci today) cannot be typed
to rename the file, and two different names can print the same. For a `HEAD`-only
name (A′), say that it is in the last commit, so the user knows a commit is needed.

**Recommended: A′ with B and escaped names.** Check listed names and `HEAD` names at
open. Keep A's error in `Repo::status` as the guard. Leave ignored files alone.

## Gaps

- fd and mdBook: not run or read `[unverified]`/`[gap]`.
- Obsidian: whether excluded files are indexed, and any behaviour for names that are
  not UTF-8 `[unverified]`/`[gap]`.
- The claim that listed names plus `HEAD` names cover every `git status` record
  (without `--ignored`) comes from the documented record kinds and one run. Exotic
  states (sparse checkout, split index, `skip-worktree`) were not tried `[unverified]`.
- The cost of `ls-tree -r HEAD` at open was not measured.
- macOS (`core.precomposeUnicode`) and Windows, where paths are not raw bytes, were
  not considered. loci's runs were Linux only.

## Sources

git v2.56.0 (docs fetched from raw.githubusercontent.com at tag v2.56.0; binary git
2.56.0):
- https://github.com/git/git/blob/v2.56.0/Documentation/config/core.adoc (lines 44–51, 152–165)
- https://github.com/git/git/blob/v2.56.0/Documentation/git-status.adoc (lines 20, 107–132, 202–203, 246, 423–431)
- https://github.com/git/git/blob/v2.56.0/Documentation/git-ls-files.adoc (lines 253–256)
- https://github.com/git/git/blob/v2.56.0/Documentation/i18n.adoc (lines 1–22)

jj v0.46.0 (7d38231; binary jj 0.46.0):
- https://github.com/jj-vcs/jj/blob/v0.46.0/lib/src/local_working_copy.rs (lines 1710–1729, 1759, 1789)
- https://github.com/jj-vcs/jj/blob/v0.46.0/lib/src/working_copy.rs (lines 236–241)
- https://github.com/jj-vcs/jj/blob/v0.46.0/lib/src/git_backend.rs (lines 1027–1028, 1180–1183)
- https://github.com/jj-vcs/jj/blob/v0.46.0/cli/src/cli_util.rs (lines 3266–3287)
- https://github.com/jj-vcs/jj/blob/v0.46.0/core/src/repo_path.rs (lines 40–44, 206–212)
- https://github.com/jj-vcs/jj/blob/v0.46.0/CHANGELOG.md (lines 430–433, under 0.44.0, 2026-08-05); issue https://github.com/jj-vcs/jj/issues/9774

ripgrep 15.2.0 (e89fff8; binary ripgrep 15.2.0; `ignore` 0.4.29, `globset` 0.4.19 in tree):
- https://github.com/BurntSushi/ripgrep/blob/15.2.0/GUIDE.md (lines 174–190)
- https://github.com/BurntSushi/ripgrep/blob/15.2.0/crates/ignore/src/walk.rs (lines 875, 900)
- https://github.com/BurntSushi/ripgrep/blob/15.2.0/crates/ignore/src/gitignore.rs (lines 259–270)
- https://github.com/BurntSushi/ripgrep/blob/15.2.0/crates/globset/src/lib.rs (lines 617–618)
- https://github.com/BurntSushi/ripgrep/blob/15.2.0/crates/printer/src/jsont.rs (lines 160–201)
- bstr 1.13.1 crate source, `src/ext_vec.rs` (lines 283–325): https://github.com/BurntSushi/bstr

cargo master 9511315a7c2b49da7f49beda9dcdd4ce6117b814 (2026-11-05 UTC, version 0.102.0):
- https://github.com/rust-lang/cargo/blob/9511315a7c2b49da7f49beda9dcdd4ce6117b814/src/sources/path.rs (lines 565–600, 702–714)
- https://github.com/rust-lang/cargo/blob/9511315a7c2b49da7f49beda9dcdd4ce6117b814/src/ops/cargo_package/mod.rs (lines 497–511, 1093–1103)
- https://github.com/rust-lang/cargo/blob/9511315a7c2b49da7f49beda9dcdd4ce6117b814/src/ops/cargo_package/vcs.rs (lines 192–245, 279–336)

zola master 48d4e7a (2026-11-03, workspace version 0.23.6; binary nixpkgs zola 0.23.6):
- https://github.com/getzola/zola/blob/48d4e7a/components/site/src/lib.rs (lines 194–253)

camino 1.2.6 (86ed283): https://github.com/camino-rs/camino/blob/86ed283/README.md (lines 15–42, 66–78)

Rust std: https://doc.rust-lang.org/std/path/struct.Path.html (`to_str`, `to_string_lossy`, `display`; fetched 2026-11-04)

Obsidian: https://obsidian.md/help/settings and
https://obsidian.md/help/Files+and+folders/Manage+notes (fetched 2026-11-04).

loci (commit 81ae7f8 plus staged changes): `crates/loci-core/src/git.rs` (lines 155,
162–183, 213–247, 302–313, 331–334, 351–380), `crates/loci-core/src/vault.rs` (lines
85, 139–160, 218–271), `crates/loci/src/commands/lint.rs` line 99,
`crates/loci-core/src/lint/disk.rs` line 43, `docs/specs/m0.md` §3 (lines 108–137),
§9.6 (line 762), §9.7 (line 796), `docs/DECISIONS.md` D133, D152, D183, D207. Runs: the
scratch repositories described above, and `target/debug/loci` built from the working
tree on 2026-11-04.
