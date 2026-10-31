# 06 — External projects: stable references and incorporation

Date: 2026-10-30. Scope: how vault notes refer to projects that live outside the vault
(code repos, paper and slide-deck projects, prototypes), so that references survive
different checkout paths and operating systems, and how much of a project the vault
should hold. Answers the **[Q] Projects** in DESIGN.md §5 and fills in the reference
format that §9 (code anchors) assumes. Claims I could not check against a primary
source are marked **[unverified]**.

**Bottom line.** Use one level of indirection, as git already does with `.gitmodules`
and `.git/config`:

- A committed `projects.toml` maps a short **project name** to a remote URL.
- An uncommitted, per-machine file maps names to checkout paths. When a path is not
  set, it is derived from the URL with a ghq-style layout (`<root>/<host>/<owner>/<repo>`).
- Notes write `proj:<name>@<commit>:<path>#L<a>-L<b>`. The commit is required wherever
  the reference is evidence and optional where it is only navigation.

Do not vendor projects or add them as submodules. A project's dated progress goes in
journal entries with a `project:` field. Its co-evolving documents (paper, slides)
stay in the project's own repo. Durable knowledge is distilled into concepts that cite
`proj:` references. The SWHID of a git commit equals its SHA-1, so archival identifiers
can be derived when needed and never have to be stored.

---

## 1. Identifier schemes

| Scheme | Form | Survives rename | Survives force-push | Private / offline |
|---|---|---|---|---|
| git URL + commit | `https://host/o/r` + 40-hex SHA | URL breaks; GitHub redirects | no, once unreachable objects are gc'd | yes / yes with a local clone |
| SWHID | `swh:1:rev:<sha>;origin=…;path=…;lines=…` | yes (intrinsic) | yes, if archived | public only / id computable offline |
| Forge permalink | `github.com/o/r/blob/<sha>/f#L10-L20` | via redirect | not guaranteed | host auth / no |
| DOI (Zenodo) | `10.5281/zenodo.N` | yes | yes (a release snapshot) | public only / no |
| SCIP symbol | `<scheme> <manager> <name> <version> <descriptors>` | the name is the package's | n/a | yes / yes |
| purl | `pkg:type/ns/name@version?q#subpath` | no | no | yes / yes |
| content hash | blob SHA, NAR hash | yes | yes | yes / yes |

**git remote + commit (lock files).** Every major package tool pins a git dependency
this way. The Nix flake lock keeps `original` (the user's intent, such as an owner
and repo or an indirect registry id) apart from `locked` (`rev`, `narHash`,
`lastModified`). Locked attributes are "final" (Nix manual 2.28, `nix flake`,
https://nix.dev/manual/nix/2.28/command-ref/new-cli/nix3-flake.html). Go writes an
unreleased commit as a pseudo-version such as `v0.0.0-20191109021931-daa7c04131f5`:
a UTC timestamp plus a 12-character commit prefix (Go Modules Reference,
https://go.dev/ref/mod). Cargo records the resolved commit of a git dependency in
`Cargo.lock` (The Cargo Book, "Specifying dependencies from git repositories",
https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html). Two lessons
for loci. First, keep the mutable locator (the URL) apart from the immutable pin (the
SHA). Second, Go found 12 hex characters plus a timestamp enough for humans; loci
should store at least 12 hex characters so that a short SHA cannot become ambiguous.

- **Rename.** A GitHub rename redirects web traffic and `git clone/fetch/push`.
  Creating a new repo under the old name breaks the redirect (GitHub Docs, "Renaming
  a repository",
  https://docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository).
  Other forges' behaviour is **[unverified]**.
- **Force-push.** After history is rewritten, the old commits remain reachable in
  clones, forks, pull requests, and "directly via their SHA-1 hashes in cached views
  on GitHub" until Support runs garbage collection (GitHub Docs, "Removing sensitive
  data from a repository",
  https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository).
  A pinned SHA can therefore resolve on one machine and not on another. A force-push
  is a real failure mode that `loci` must detect, not assume away.

**SWHID.** SoftWare Hash IDentifiers have the form `swh:1:<type>:<sha1>`, with types
`cnt`, `dir`, `rev`, `rel` and `snp`. The qualifiers `origin`, `visit`, `anchor`,
`path`, `lines` and `bytes` are separated by `;`, for example
`…;origin=https://…;anchor=swh:1:rev:…;path=/Examples/…;lines=9-15` (SWHID spec
v1.2, clauses 5–6, https://www.swhid.org/specification/v1.2/). SWHID was published as
**ISO/IEC 18670:2025** on 2025-04-23
(https://www.swhid.org/news/2025-04-23-swhid-standardized-as-iso-iec-18670/). The
SWHID.org FAQ says "18670:2024", which conflicts with the news post and the Software
Heritage anniversary post
(https://www.softwareheritage.org/2026/04/23/one-year-swhid-iso-iec-18670-standard/);
2025 is the majority reading. For `cnt`, `dir`, `rev` and `rel`, the hash equals the
Git object id. Spec v1.2 §5.8 calls this compatibility "incidental" and does not
guarantee it for future versions or for SHA-256 Git. Resolution goes through the
Software Heritage archive, and Save Code Now accepts only publicly accessible
repositories (SWH FAQ, https://docs.softwareheritage.org/user/faq/index.html). For
loci, a `swh:1:rev:` id is free to derive from a pinned SHA and gives a public
project an archival fallback when its forge disappears. It does nothing for a
private project.

**Forge permalinks** replace the branch with a commit SHA. GitHub's `y` key rewrites
the URL in place (GitHub Docs, "Getting permanent links to files",
https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files).
They suit a human with a browser, but they tie the reference to one host and add
nothing that URL + SHA + path does not already hold. `loci` can generate them
instead of storing them.

**DOIs for software.** With the Zenodo–GitHub integration, each GitHub release gets
a version DOI, and a concept DOI stands for all versions (OpenAIRE blog, 2017,
https://www.openaire.eu/community/blogs/zenodo-now-supports-doi-versioning-1). It
works for public repos only, and secondary guides report that `.zenodo.json` takes
precedence over `CITATION.cff` **[unverified against Zenodo docs; docs host
unreachable]**. CITATION.cff 1.2.0 has `repository-code`, `commit`, and
`identifiers` entries of type `doi` or `swh`
(https://github.com/citation-file-format/citation-file-format/blob/main/schema-guide.md).
DOIs cite releases, while notes cite working commits, so a DOI is metadata a project
may have, not a reference loci should require.

**SCIP symbols.** Grammar: `<scheme> ' ' <manager> ' ' <package-name> ' ' <version>
' ' (<descriptor>)+ | 'local ' <local-id>`, with `.` for an empty field and a doubled
space to escape a space (`scip.proto`, https://github.com/sourcegraph/scip/blob/main/scip.proto).
A symbol names something inside a package. It does not say where the repo lives, so
it pairs with a project reference and cannot replace one. DESIGN §9 already uses it
that way.

**purl** (`pkg:type/namespace/name@version?qualifiers#subpath`) is standardised as
ECMA-427 (pointer in https://github.com/package-url/purl-spec). It identifies
*packages* for SBOMs and vulnerability matching. I could not fetch the current type
list to confirm the `github`/`generic` types and the `vcs_url` qualifier
**[unverified]**. It is the wrong grain for "line 40 of a file at a work-in-progress
commit".

**URNs and content hashes.** A private URN namespace (`urn:x-…`) adds syntax without
resolution. Content hashes (blob SHA, the NAR hash) survive everything but identify
bytes, not "the file the note is about". They are useful as drift detectors (the
range hash in DESIGN §9), not as names.

**Conclusion.** Name the project with a vault-local short name. Locate it with a
mutable URL kept in one place. Pin evidence with a full or ≥12-hex commit SHA, and
optionally a path and line range. Derive SWHIDs and permalinks; never store them.

## 2. Indirection for machine-specific paths

Every mature multi-repo tool separates a shared declaration from a local binding:

- **git submodules.** `.gitmodules` "suggest[s] defaults for the upstream collection
  of repositories". `$GIT_DIR/config` holds the per-clone values and overrides the
  URL. The tree stores only a gitlink, "the object name of the commit that the
  superproject expects" (gitsubmodules(7), https://git-scm.com/docs/gitsubmodules).
- **Google `repo`.** `<remote fetch=…>`, `<default revision=…>` and `<project name
  path revision>` live in a shared manifest. Per-user additions go in
  `.repo/local_manifests/*.xml` without editing it (manifest-format.md,
  https://gerrit.googlesource.com/git-repo/+/HEAD/docs/manifest-format.md).
- **myrepos.** `~/.mrconfig` has one section per checkout path with a `checkout =
  git clone …` command (https://myrepos.branchable.com/). The path is the key, so
  the file is per-machine by construction.
- **Nix.** The flake registry maps indirect ids such as `nixpkgs` to locations, and
  `flake.lock` pins them (Nix manual above). The registry's global, user and system
  levels are described in `nix registry`, which I did not fetch **[unverified]**.
- **ghq / GOPATH.** `ghq get` clones to `<root>/<host>/<owner>/<repo>`. `ghq.root`
  in git config (repeatable, with the last value primary), `ghq.<url>.root` and
  `GHQ_ROOT` set the root (https://github.com/x-motemen/ghq). GOPATH mode stored
  `example.com/repo/sub` at `$GOPATH/src/example.com/repo/sub` (Go Modules
  Reference). With a canonical layout, the only machine-specific value is the root,
  and every path follows from the URL.
- **direnv** binds environment variables per directory. It suits per-machine roots
  but adds a dependency for one variable.

For loci: commit the name → URL map, and keep a per-machine file that holds a root
and, rarely, an explicit path for a checkout outside the layout. An explicit path
handles macOS vs Linux home directories and old checkouts. The derived layout makes
the common case zero-config.

## 3. Incorporation approaches

| Approach | Vault size | Drift | Privacy | Agent access | Fits |
|---|---|---|---|---|---|
| Submodule | small, but the gitlink churns | pinned; bumps are commits | the URL is exposed to vault readers; the sub repo needs its own auth | needs `submodule update` | none here |
| Subtree / vendor | grows with the history | frozen copy | a private project ends up inside the vault, or the reverse | trivial | tiny, dead upstreams |
| Sparse mirror / worktree | local only | live | none | trivial | (local mechanism only) |
| Vendored docs | moderate | stale copy | as for vendoring | trivial | external manuals, already `sources/docs/` |
| Distill only | none | concepts can go stale silently | best | none | others' code read once |
| Link (manifest + refs) | none | detectable (`code drift`) | the URL is in the private vault only | `git show` at a pinned commit | default |
| Two-way pointer | none | n/a | a public repo reveals that the vault exists | — | only if something reads it |
| Co-located docs-as-code | none in the vault | none | follows the project | needs the checkout | paper and slide projects |

Notes:

- **Submodules** make every bump of a cited project a vault commit. They need
  recursive clone and auth on every machine, and the git commands agents run become
  harder to get right. loci's references carry their own commits, so a single
  superproject pin adds nothing. DataLad (below) shows that this model works at
  scale, but only with dedicated tooling.
- **Distill only** is what the vault already does for papers, and the paper stays as
  evidence. For code, the equivalent of keeping the PDF is the pinned reference.
  Without it, a concept about a codebase has no evidence, which breaks D5 ("concepts
  may cite only sources, code anchors or commits").
- **Two-way links** (a `.loci` file or `notes/` directory in the project) leak the
  vault's existence when the project is public. No `loci` command would read the
  file (principle 5). Skip them until a procedure needs to find vault notes from
  inside a project checkout. Even then, `loci backlinks proj:<name>` run from the
  vault answers that.
- **Co-evolution vs distillation.** When notes and code change together (your own
  prototype, a paper draft), the project's docs belong in the project, under review
  with its code. The vault records dated progress (journal) and distilled concepts.
  When knowledge is only extracted from someone else's code, the vault holds the
  concepts plus pinned references, and nothing else.

## 4. Prior art in PKM and research tools

- **Obsidian.** Cross-vault links exist only as URIs:
  `obsidian://open?vault=<name or 16-char id>&file=<path>`
  (https://obsidian.md/help/Extending+Obsidian/Obsidian+URI). The vault name is a
  machine-local indirection, the same pattern this note recommends, but it covers
  only Obsidian vaults.
- **Logseq** supports multiple graphs. I found no primary documentation of
  cross-graph or external-folder links **[unverified]**.
- **org-mode link abbreviations.** `[[linkword:tag]]` expands through
  `org-link-abbrev-alist` or a buffer-local `#+LINK:`, with `%s`, `%h` or a Lisp
  function (Org Manual, "Link Abbreviations",
  https://orgmode.org/manual/Link-Abbreviations.html). This is the closest syntax
  precedent: a short committed prefix that per-user config resolves.
- **Zotero linked files.** Inside the "Linked Attachment Base Directory", Zotero
  "stores a relative path to that base directory", and each computer sets its own
  base (Zotero docs, https://www.zotero.org/support/preferences/advanced). It is the
  same root-plus-relative-path split, and it is proven across machines and operating
  systems.
- **Quarto/Jupyter projects** treat a directory with `_quarto.yml` as the unit. That
  is docs-as-code, consistent with keeping paper and slide projects in their own
  repos.
- **DataLad** is the strongest prior art. Subdatasets are registered in
  `.gitmodules` with `path`, `url`, `datalad-id` and `datalad-url`. The superdataset
  "only stores *which version* of the subdataset is currently used" (DataLad
  Handbook, "Nesting",
  https://handbook.datalad.org/en/latest/basics/101-106-nesting.html).
  `datalad get -n` installs a subdataset without its annexed data, and file content
  comes through git-annex (https://docs.datalad.org/en/stable/generated/man/datalad-get.html).
  Two ideas transfer: a stable dataset id that is independent of URL and path (loci's
  project name), and lazy materialisation (loci's "check out only when needed").
  git-annex and the superproject pins do not transfer: they cost too much for
  references that already carry commits.
- **RO-Crate.** A relative `@id` must be present in the crate, while an absolute URI
  makes a web-based entity, which "SHOULD" carry `sdDatePublished` to record when it
  was accessed (RO-Crate 1.2, Data Entities,
  https://www.researchobject.org/ro-crate/specification/1.2/data-entities.html; 1.3
  is now the newest release). The local-vs-remote split matches the manifest
  approach.
- **Research compendium.** Marwick, Boettiger & Mullen, "Packaging Data Analytical
  Work Reproducibly Using R (and Friends)", *The American Statistician* 72(1):80–88,
  2018, https://doi.org/10.1080/00031305.2017.1375986. Their convention is one repo
  per project holding the paper, code, data and environment. This supports keeping a
  paper project out of the vault.

## 5. Recommended design for loci

### 5.1 Committed manifest: `projects.toml` (vault root)

```toml
[solver]                                 # the name: permanent, used in every reference
url = "https://github.com/example/solver"  # mutable; edit on rename or move

[thesis-slides]
url = "git@git.example.org:me/thesis-slides.git"
```

`url` is the only field. Its readers are path derivation, `loci proj clone`, permalink
generation and SWHID `origin=`. No default branch: `loci code drift` compares against
the remote `HEAD`, and a `branch` field is added only if a project needs a
non-default branch. No pinned commit: each reference carries its own (§5.3), as
DESIGN §9 anchors already do. A manifest-wide pin would be a second source of truth.
No visibility field: nothing reads it. Renaming a project means editing `url`; the
name stays. `loci mv` already rewrites links, so renaming the *name* is a `loci mv`
case.

### 5.2 Per-machine mapping: `$XDG_CONFIG_HOME/loci/projects.toml`

```toml
root = "~/src"                 # path = <root>/<host>/<owner>/<repo>, derived from url
[paths]
thesis-slides = "~/src/talks/thesis-slides"   # explicit override
```

This file is outside the vault and never committed. It cannot go in `.loci/`,
because `.loci/` is a rebuildable cache and this mapping cannot be rebuilt.
Resolution is explicit path, then derived path, then an error that names both
candidates. `~` and the host and owner are expanded per machine, so the same vault
works on macOS and Linux. ghq users set `root` to their `ghq root`.

### 5.3 Reference syntax

```
proj:<name>[@<sha>][:<path>][#L<a>[-L<b>]]
```

- In frontmatter `sources:`, next to `commit:` refs:
  `"proj:solver@3cf424a91be2:src/search/heap.rs#L40-L88"`. Here `@<sha>` is
  **required**, because evidence must be pinned.
- In the body, as a markdown link:
  `[heap](proj:solver@3cf424a91be2:src/search/heap.rs#L40)`. A `@` is optional
  for navigation and resolves against the working tree. It does not go in a
  wikilink, because markdown-oxide would report `[[proj:…]]` as dangling.
- Symbol-level anchors stay in the DESIGN §9 shape (SCIP symbol + commit + range
  hash), with `proj:<name>@<sha>` as the repo part.
- Journal entries about a project get `project: <name>` in frontmatter. It is read
  by `loci recent --project` and the per-project timeline view. Streams stay
  life-domain (`research`, `life`), so a project is a field and not a stream. This
  answers the DESIGN §5 [Q]: neither option as posed, but its "own repo" half for
  the documents.

### 5.4 What `loci` checks

| Check | Needs the checkout? | Severity |
|---|---|---|
| unknown project name | no | error |
| `sources:` ref without `@sha`, or sha < 12 hex | no | error |
| sha ambiguous or absent in the checkout | yes | error (suggest `git fetch`; if still absent: force-push loss) |
| path absent at sha, or line range out of bounds | yes | error |
| checkout missing on this machine | — | listed in `loci lint` output as "N refs unchecked (no checkout: solver)"; `--strict-projects` makes it an error |
| body ref without sha whose path no longer exists in HEAD | yes | warning |

The first two checks run anywhere, including on the phone and in CI. The rest run
where a checkout exists, and lint always says what it skipped, so nothing passes
silently. Resolving a pinned ref never touches the working tree: it runs
`git -C <path> cat-file -e <sha>:<path>`.

### 5.5 How agents read projects

- `loci proj path <name>` prints the resolved checkout path.
- `loci proj show <ref>` runs `git show <sha>:<path>` and slices the line range, so
  the agent reads what the note cites, not whatever the branch holds now.
- `loci proj clone <name>` clones into the derived path, the one write command.
- Agents must not edit project checkouts during vault jobs; `loci accept` commits
  only vault paths. Work on a project is a separate session in that repo.

### 5.6 Kinds of project

- **Someone else's code (project as source).** Add a manifest entry and distill
  concepts that cite pinned refs. `loci code drift` (M5) flags concepts whose cited
  ranges changed. No `sources/code/` copy.
- **Your own prototype (project as journal subject).** Progress and results go in
  journal entries with `project:`. Techniques are distilled into concepts. Design
  docs that change with the code stay in the repo.
- **Paper or slide projects (separate repo with pointer).** These are their own
  repos (a research compendium). The vault holds the manifest entry, journal entries,
  and concepts that cite `proj:` refs to sections of the draft.

**Open [Q]s.**

- Should `loci` create `refs/loci/<sha>` in your own checkouts so that cited commits
  survive local gc after a rebase? This guards only the local copy, not the remote.
- Should public projects record a SWHID `snp` at citation time via Save Code Now?
  That needs network access and applies only to public repos.
