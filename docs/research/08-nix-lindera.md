# Building lindera dictionaries inside the Nix sandbox

Question: can a Rust crate that uses `lindera` 6.2 with `embed-ipadic` and
`embed-ko-dic`, plus `jieba-rs` 0.11 with its default dictionary, build with
`rustPlatform.buildRustPackage` in the Nix sandbox, where the build has no network?

**Answer: yes.** I tested it on 2026-10-30 with Nix 2.34.9 (sandbox on), nixos-unstable
`1c89083e`, and rustc 1.99.0. Fetch each dictionary archive with `fetchurl`. Then copy the
archives into a writable directory and point `LINDERA_BUILD_DICTIONARY_CACHE_DIR` at it.
nixpkgs uses the same method for `lrcget`.

## 1. How lindera 6.2.0 fetches dictionaries

Each dictionary crate has a small `build.rs`. It passes a `FetchParams` to
`lindera_dictionary::assets::build_embedded_dictionary`.

`lindera-ipadic-6.2.0/build.rs`:
```rust
file_name: "mecab-ipadic-2.7.0-20250920.tar.gz",
download_urls: &["https://Lindera.dev/mecab-ipadic-2.7.0-20250920.tar.gz"],
md5_hash: "a95c409f12f1023fce8ef91f991ef042",
```
`lindera-ko-dic-6.2.0/build.rs`:
```rust
file_name: "mecab-ko-dic-2.1.1-20180720.tar.gz",
download_urls: &["https://Lindera.dev/mecab-ko-dic-2.1.1-20180720.tar.gz"],
md5_hash: "b996764e91c96bc89dc32ea208514a96",
```

The shared logic is in `lindera-dictionary-6.2.0/src/assets.rs`:

- `CACHE_DIR_ENV = "LINDERA_BUILD_DICTIONARY_CACHE_DIR"`. The deprecated alias
  `LINDERA_DICTIONARIES_PATH` is scheduled for removal in v7 (lines 335–367).
  Older lindera versions used `LINDERA_CACHE`; nixpkgs' `pg_search` still sets it for lindera 1.5.
- If the variable is set, `fetch()` uses
  `cache_dir.join(format!("{pkg_version}-fmt{DICTIONARY_FORMAT_VERSION}"))` as the build
  directory. If it is not set, `fetch()` uses `OUT_DIR` (lines 533–572). Here `pkg_version` is
  the `CARGO_PKG_VERSION` of the **dictionary crate**, for example `lindera-ipadic` 6.2.0.
  `DICTIONARY_FORMAT_VERSION` is 2 (`src/dictionary/metadata.rs:47`). The resulting
  subdirectory is `6.2.0-fmt2`.
- Fast path: if the cache already contains a built dictionary whose `metadata.json` matches,
  `fetch()` returns without doing anything (line 598).
- If `DOCS_RS` is set, lindera builds a **dummy** one-entry dictionary (line 605). This avoids
  the download, but the resulting binary is useless.
- Otherwise, if `<build_dir>/<file_name>` exists and its MD5 matches, lindera skips the
  download (lines 610–660). If the MD5 does not match, lindera deletes the file and downloads
  it again. lindera then extracts the archive, builds the dictionary, and writes the output
  into the same directory. **The directory must be writable.**
- `build_embedded_dictionary` does nothing when the embed feature is off and no cache
  variable is set (line 836).
- The build script emits `cargo::rustc-env=LINDERA_WORKDIR=<build_dir>`. The dictionary is
  embedded at compile time, so the binary does not need the directory at runtime. Nix's
  "references to /build/" check passes.

lindera 6.2.0 has no offline flag and no feature that skips the download, apart from the
cache directory and `DOCS_RS`.

## 2. jieba-rs 0.11

jieba-rs downloads nothing at build time. `jieba-rs-0.11.0/src/data/{dict.txt,idf.txt,posseg.txt}`
are part of the published crate (about 5 MB, 6 MB, and 2.5 MB). The `default-dict` feature
embeds them with `include-flate`, and the crate has no `build.rs`. `cargoLock.lockFile` alone
is enough.

## 3. Nix build (tested)

The scratch crate was the earlier probe crate (tantivy 0.27, lindera 6.2.0, jieba-rs 0.11.0).
It was built with features `ipadic ko jiebars` and a small `main.rs` that segments one string.

- **Naive build** (no cache variable): fails at the `lindera-ipadic` build script with
  `Failed to download dictionary assets … Failed to download a valid file from all sources`.
- **Fixed build** (below): succeeds. It took about 49 s wall time on 16 cores, with sources
  already in the store, including `checkPhase`. The output is a 131 MiB stripped binary
  (closure 178 MiB). Its only runtime references are glibc and gcc-lib.

Output of the resulting binary:
```
ja 形態素解析は自然言語処理の基礎です → 形態素 | 解析 | は | 自然 | 言語 | 処理 | の | 基礎 | です
ja 東京都に住んでいます           → 東京 | 都 | に | 住ん | で | い | ます
ko 경사하강법과 신경망을 공부합니다     → 경사 | 하강법 | 과 | 신경망 | 을 | 공부 | 합니다
zh 我在学习倒排索引和神经网络 (jieba cut_for_search, HMM off)
   → 我 | 在 | 学习 | 倒排 | 索引 | 和 | 神经 | 网络 | 神经网 | 神经网络
```

```nix
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};

      # Archives that lindera-{ipadic,ko-dic} 6.2.0 build.rs would download.
      # File names and MD5s are in each crate's build.rs; lindera re-checks the MD5.
      ipadic = pkgs.fetchurl {
        url = "https://Lindera.dev/mecab-ipadic-2.7.0-20250920.tar.gz";
        hash = "sha256-p7qfZF/+cJTlauHEqB0QDfj7seKLvheSYi6XKOFi2z0=";
      };
      ko-dic = pkgs.fetchurl {
        url = "https://Lindera.dev/mecab-ko-dic-2.1.1-20180720.tar.gz";
        hash = "sha256-cCztIcYWfp2a68Z0q17lSvWNREOXXylA030FZ8AgWRo=";
      };

      probe = { withDicts }: pkgs.rustPlatform.buildRustPackage ({
        pname = "lindera-probe";
        version = "0.1.0";
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;
        buildNoDefaultFeatures = true;
        buildFeatures = [ "ipadic" "ko" "jiebars" ];
      } // pkgs.lib.optionalAttrs withDicts {
        # The cache key is "<dictionary crate version>-fmt<DICTIONARY_FORMAT_VERSION>"
        # (lindera-dictionary src/assets.rs). The directory must be writable:
        # lindera extracts and builds the dictionary inside it.
        preBuild = ''
          export LINDERA_BUILD_DICTIONARY_CACHE_DIR=$TMPDIR/lindera-cache
          mkdir -p $LINDERA_BUILD_DICTIONARY_CACHE_DIR/6.2.0-fmt2
          cp ${ipadic} $LINDERA_BUILD_DICTIONARY_CACHE_DIR/6.2.0-fmt2/mecab-ipadic-2.7.0-20250920.tar.gz
          cp ${ko-dic} $LINDERA_BUILD_DICTIONARY_CACHE_DIR/6.2.0-fmt2/mecab-ko-dic-2.1.1-20180720.tar.gz
        '';
      });
    in {
      packages.${system} = {
        naive = probe { withDicts = false; };
        default = probe { withDicts = true; };
      };
    };
}
```

The variable is exported in `preBuild`. All stdenv phases run in the same shell, so
`checkPhase` sees it too. The fixed build ran its `checkPhase` and passed. In that phase the
build script took the cache fast path.

## Caveats

- **Version coupling.** The `6.2.0-fmt2` subdirectory, the archive names, and the hashes all
  depend on the locked dictionary-crate versions. Recheck them whenever a lindera bump changes
  `Cargo.lock`. A mismatch fails loudly, not silently: lindera finds no archive, tries to
  download it, and the sandbox makes that fail. If `lindera-ipadic` and `lindera-ko-dic` ever
  have different versions, each one needs its own subdirectory.
- **Do not use `DOCS_RS=1` as a shortcut.** The build succeeds but embeds dummy dictionaries.
- Copy the archives with `cp` and do not symlink them. lindera only reads them, but it may
  delete an archive whose MD5 does not match, and it writes build output next to them.
- Binary size: about 131 MiB with ipadic, ko-dic, and jieba. This matches the 143 MB noted in
  `07-cjk-tokenization.md`, which included tantivy.
- The `Lindera.dev` host is a single point of failure for `fetchurl`. After the first fetch,
  the archives live in the Nix store and in binary caches.

## Alternatives

- **nixpkgs precedent.** No lindera dictionary package exists in nixpkgs. Three packages
  handle lindera directly:
  - `pkgs/by-name/lr/lrcget/package.nix` uses the same method for lindera 5.3:
    `LINDERA_BUILD_DICTIONARY_CACHE_DIR` and `5.3.0-fmt2` in `preConfigure`.
  - `pkgs/servers/sql/postgresql/ext/pg_search.nix` uses the older `LINDERA_CACHE` variable
    with lindera 1.5.1.
  - `pkgs/by-name/pa/pagefind/package.nix` patches the vendored `build.rs` URL to
    `file://${src}` for lindera 0.32.

  The pagefind method depends on the HTTP client accepting `file://`. I did not test it with
  lindera 6.x, which uses ureq 3 [unverified]. The cache-directory method is simpler.
- **Vendor the archives into the repository.** This adds about 60 MB to git, and the build
  still needs the same cache-directory wiring. It is no better than `fetchurl`.
- **`[patch.crates-io]` fork of the dictionary crates** that embed prebuilt data. This avoids
  the variable, but the fork must be maintained on every lindera release. It is not worth it.
- **crane.** The same `preBuild` export works. Set it on both `buildDepsOnly` and
  `buildPackage`, because the dependency-only build runs the lindera build scripts
  [not tested]. Another option is to build the cache directory once in its own derivation and
  copy it in, so that the 2-pass build does not build the dictionaries twice [not tested].
