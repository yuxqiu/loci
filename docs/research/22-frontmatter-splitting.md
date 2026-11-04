# 22 — How markdown tools find and split YAML frontmatter

Status: research, 2026-11-04. Background for a possible change to how loci finds a
note's frontmatter. Builds on D244 and D245 (the current rule) and m0 §4.3; cited, not
repeated.

Question: how do other projects detect a YAML frontmatter block and split it from the
markdown, and what exactly are their rules? Covered: what opens a block, what closes it,
whether `...` closes it, trailing whitespace, BOM and CRLF, an unclosed block, an empty
block, what must follow the closing line, whether the content must be a mapping, and
whether the split happens before markdown parsing or inside the parser.

What loci has today (working tree at 086eebc, 2026-11-04):

- **Frontmatter is pulldown-cmark's metadata block at offset 0.** `metadata_block`
  runs its own pulldown-cmark 0.13.6 read of the file (after a BOM) with
  `ENABLE_YAML_STYLE_METADATA_BLOCKS` on, and keeps the first event only if it is a
  `MetadataBlock` starting at 0 (`crates/loci-core/src/parse/frontmatter.rs` lines
  93–105).
- **Two patches.** An empty block (`---`, lines of only spaces and tabs, `---`), which
  pulldown does not read, is frontmatter with no fields (`empty_frontmatter`, lines
  107–124). If neither applies and the first line is `---` with optional trailing spaces
  and tabs (`fence`, lines 126–130), the note is `fm.unterminated` (lines 77–80).
- **The body is walked from the end of the block with the metadata option off,**
  because pulldown otherwise reads `---`, text, `---` in the middle of a document as a
  metadata block (D244).
- **YAML.** `parse_yaml` keeps the first YAML document and requires a mapping
  (lines 336–350). No document (empty, blank or comment-only text) gives no fields.

The proposal being evaluated: split the frontmatter in loci before markdown parsing.
The first line is `---` (after an optional BOM). The block closes at the next `---`
line. `...` does not close it. An unclosed block is `fm.unterminated`. pulldown's
metadata option is turned off everywhere.

Conventions as in research 12 to 21: `[unverified]` means I did not confirm the claim
in a primary source, `[design inference]` is my own extrapolation, and `[gap]` means I
looked and found nothing. Source files were read at the tags and commits listed under
Sources, and line numbers refer to those. "From the code" means I read the code but did
not run it. Only pulldown-cmark and loci were run (throwaway tests in a copy of the
repo, pulldown-cmark 0.13.6 from `Cargo.lock`). Examples are neutral (`id: a`, `x`).

## TL;DR

| Project | Open rule | Close rule | `...` closes | Trailing whitespace on fences | BOM | Unterminated | Empty block (`---` `---`) | Where split happens | Source |
|---|---|---|---|---|---|---|---|---|---|
| pulldown-cmark 0.13.6 | exactly `---` at **any** block start, unindented; next line not blank and not a close | exactly `---` or `...`, unindented, then EOL or EOF | yes | open: any ASCII whitespace; close: spaces only | not stripped (read as text) | no block; read as markdown | no block (two rules) | inside the parser | `scanners.rs` 533–552, 744–797; `firstpass.rs` 319–331, 1421–1459 |
| Jekyll 4.4.1 | file start, `---`, then `\s*\n` | first line `---` or `...`, then `\s*` | yes | yes (`\s*`) | stripped (`bom\|utf-8` read) | no match: no front matter | yes, data `{}` | before markdown (regexp on the file) | `document.rb` 13; `convertible.rb` 37–68; `utils.rb` 141–145, 305–314 |
| Hugo 0.169.0 | first non-whitespace character `-`; must be `---` or it is an error | a line that **starts with** `---` | no | open: rest of line kept as YAML; close: rest of line left in body | no handling found `[gap]` | error "EOF looking for end YAML front matter delimiter" | yes | before markdown (page lexer) | `pagelexer_intro.go` 16–40, 122–159 |
| gray-matter (JS) 4.0.3 | file starts with `---`, 4th character not `-`; rest of line is a language name | first `\n---` substring (not line-anchored) | no | open: language name trimmed; close: rest of line goes to content | stripped | the rest of the file is the matter; content `''` | yes, `isEmpty`, data `{}` | before markdown (library) | `index.js` 57–131, 206–219; `lib/to-file.js` 38–39; `lib/utils.js` 44–48 |
| `gray_matter` (Rust) 0.3.2 | first line, `trim_end`, equals `---` | next line, `trim_end`, equals `---` | no | yes (Unicode whitespace) | not stripped | no data; content is the file minus the opening line | yes, no data | before markdown (library) | `src/matter.rs` 54–131 |
| python-frontmatter 1.3.0 | after `.strip()` of the whole text, a line of **3 or more** `-`, then `\s*` | next such line | no | yes | not handled in code `[unverified]` | `ValueError` caught: no metadata, whole text | yes, YAML `None` ignored | before markdown (library) | `default_handlers.py` 196–215, 246–260; `__init__.py` 49–97 |
| Pandoc 3.13 | `---` + spaces/tabs + newline at any block start (after a blank line or at file start); next line not blank | `---` or `...` + spaces/tabs + newline | yes | spaces and tabs | dropped at decode | not a block; read as markdown | yes (from the code) | inside the parser | `Readers/Metadata.hs` 162–181; `Readers/Markdown.hs` 311–316, 486–490; `UTF8.hs` 98–103; MANUAL |
| comrak 0.59.0 | after BOM(s), exactly the delimiter then LF or CRLF | first `\n<delim>\r\n`, else `\n<delim>\n`, else `\n<delim>` then EOF or a newline | no | none | stripped | no front matter | **no** (from the code) | before markdown (`parse` splits first) | `strings.rs` 373–418; `parser/mod.rs` 307–314 |
| markdown-rs 1.0.0 | document start after optional BOM; exactly `---` (or `+++`) + spaces/tabs + EOL | exactly the same 3 markers + spaces/tabs, then EOL or EOF | no | spaces and tabs | stripped | not frontmatter; read as markdown | yes | inside the tokenizer, document start only | `construct/frontmatter.rs` 1–290; `construct/document.rs` 51–85 |
| micromark-extension-frontmatter 2.0.0 | line 1 column 1 (anywhere with `anywhere: true`); exactly `---` + spaces/tabs + EOL | same as markdown-rs | no | spaces and tabs | stripped by micromark | not frontmatter | yes | inside the tokenizer | `dev/lib/syntax.js` 94–388; micromark `preprocess.js` 50–54 |
| markdown-it-front-matter 0.2.4 | line 0, byte 0 is `-`, 3 or more `-`; rest of the line ignored | a line of at least as many `-` + spaces/tabs, indent under 4; or the line after a `...` line | yes | close: spaces/tabs | not handled `[unverified]` | the block runs to the end of the document | yes | inside the parser (block rule) | `index.js` 10–128 |
| Obsidian | "Type `---` at the very beginning of a file" | not stated | not stated | not stated | not stated | not stated | not stated | closed source | help page only; rest `[unverified]` |

Two things nearly all of them share: the opening line is the first line of the file,
and the closing line is three dashes alone on a line. The differences are in `...`,
whitespace, what an unclosed block does, and whether a block may appear later in the
document.

## 1. pulldown-cmark 0.13.6

Source: the crate as published (`.cargo_vcs_info.json` gives commit 014b61d38d).

**Where it scans.** `FirstPass::parse_block` (`src/firstpass.rs` line 84) runs once for
each new block. After blank lines and indented code it checks for a metadata block,
guarded only by `indent == 0` (lines 319–331). Nothing checks that this is the first
block of the document. That is why a block can appear in the middle of a document: any
place where a new block can start, an unindented `---` line is a candidate.

**Open** (`scan_metadata_block`, `src/scanners.rs` lines 744–797): the line starts with
exactly three `-` (line 763 counts every repeated `-`, and line 771 requires the count
to be 3). The rest of the line must be ASCII whitespace (`is_ascii_whitespace`, lines
765–770), which includes tabs. The scan then looks ahead for a closing line. The first
content line must be neither a closing line nor blank (lines 778–785), so pulldown never
reads an empty block or a block whose first content line is blank. With no closing line
before the end of the input, there is no block (line 790).

**Close** (`scan_closing_metadata_block`, lines 533–552): exactly three `-`, or exactly
three `.` for a YAML-style block (lines 539–546), then **spaces only** (line 550), then a
line end or end of input (`scan_eol`, lines 493–501, which also treats a lone CR as a
line end). The opening line accepts tabs but the closing line does not. `parse_metadata_block`
(`firstpass.rs` lines 1421–1459) also requires the closing line to be unindented (line
1442).

**What follows the block.** Nothing is required. Trailing whitespace after the closing
line is consumed (line 1458), and the next line starts a new block, which can be
another metadata block.

**Observations** (run on 0.13.6 with only `ENABLE_YAML_STYLE_METADATA_BLOCKS`, and again
with loci's full option set; same results):

| Input | Events |
|---|---|
| `---\nid: a\n---\n\n---\nx\n---\n` | block 0..13, then a second block 15..24 |
| `---\nid: X\n---\n---\nx\n---\n` | block 0..13, then a second block 14..23 |
| `para\n\n---\nx\n---\n` | paragraph, then a block 6..15 |
| `para\n---\nx\n---\n` | two setext headings (the `---` lines are inside a paragraph, which `parse_paragraph` handles, so `parse_block` never sees them) |
| `- a\n---\nx\n---\n` | a list, then a block 4..13 |
| `---\nid: a\n---\t\nbody\n` | rule, setext heading, paragraph (tab not allowed on the close) |
| `--- \nid: a\n---\n`, `---\t\nid: a\n---\n` | block (whitespace allowed on the open) |
| `---\nid: a\n...\nbody\n` | block 0..13, closed by `...` |
| `\u{feff}---\nid: a\n---\n` | setext heading whose text starts with the BOM |
| `---\n---\n`, `---\n\n---\n` | two rules |
| `---\n\nid: a\n---\n` | rule, heading (blank first content line) |
| `---\nid: a\n` | rule, paragraph |
| `---\nid: a\n----\nb\n---\n` | one block 0..20 (`----` is content) |

The first row reproduces the observation in D244. The second does not: at 0.13.6 the
input `---\nid: X\n---\n---\nx\n---\n` gives a metadata block at 0..13, and loci's
current parser reads `id: X` from it (checked). The reported "no metadata block" for
that input may come from the walk before D244, which kept the last block it saw
`[design inference]`.

## 2. Jekyll 4.4.1

The rule is one regular expression, `YAML_FRONT_MATTER_REGEXP =
%r!\A(---\s*\n.*?\n?)^((---|\.\.\.)\s*$\n?)!m` (`lib/jekyll/document.rb` line 13). In
Ruby, `/m` makes `.` match newlines and `^`/`$` are line anchors.

- **Open:** `\A---` at byte 0, then `\s*\n`. `\s` covers `\r` and blank lines, so CRLF and
  trailing whitespace pass. `----` does not open.
- **Close:** the first line (lazy `.*?`) that is `---` or `...` followed by `\s*`.
- **Split:** before markdown. `read_content` (lines 482–487) and `Convertible#read_yaml`
  (`convertible.rb` lines 37–46) match the file, pass capture 1 to `SafeYAML.load`, and
  keep `post_match` as the content. Capture 1 includes the opening `---` line, which
  YAML reads as a document start marker.
- **Unterminated:** no match, so no front matter is read, and the whole file stays the
  content.
- **Empty:** matches; `SafeYAML.load` returns nil, and `self.data ||= {}` (line 55).
- **Mapping:** pages raise `InvalidYAMLFrontMatterError` when the data is not a Hash
  (`convertible.rb` lines 64–68).
- **BOM:** files are read with `bom|utf-8` when the encoding is UTF (`utils.rb` lines
  305–314).
- Whether a file is processed at all depends on `has_yaml_header?`, which only checks
  `\A---\s*\r?\n` on the first line (`utils.rb` lines 141–145).

## 3. Hugo 0.169.0

`lexIntroSection` (`parser/pageparser/pagelexer_intro.go` lines 16–40) skips ASCII
whitespace and line ends, then dispatches on the first other character: `-` is YAML,
`+` TOML, `{` JSON, `#` Org.

- **Open:** after the first `-`, the next two characters must be `-`, or the lexer
  returns the error "invalid YAML delimiter" (lines 124–128). From the code, this
  includes a file whose first non-blank character starts a `- item` list `[unverified]`
  (not run). Leading blank lines before `---` are accepted.
- **Close:** at each line start, `hasPrefix("---")` (lines 145–152). The lexer then
  skips the three dashes and one line break. A longer line such as `----` closes too,
  and its extra characters stay in the body (from the code). `...` is not a close.
- **Trailing whitespace:** on the opening line, the characters after `---` become part of
  the front matter text (the `ignore` call at line 133 comes before them). YAML ignores
  trailing spaces, so `--- ` works.
- **Unterminated:** an error, "EOF looking for end YAML front matter delimiter"
  (line 141).
- **Empty:** `---\n---` emits an empty front matter item (lines 131, 145–147).
- **BOM:** nothing in `parser/pageparser` handles it `[gap]`; elsewhere `[unverified]`.
- Whether the decoded value must be a map is not covered here `[unverified]`.

## 4. gray-matter (JavaScript) 4.0.3 and `gray_matter` (Rust) 0.3.2

**gray-matter 4.0.3** (npm `gitHead` e54a33b394), `parseMatter` in `index.js` lines
57–131:

- **Open:** the string starts with the open delimiter `---` (line 69), and the next
  character is not `-` (line 77). The rest of the first line is read as a language name
  (`matter.language`, lines 206–219). A blank name means the default, YAML.
- **Close:** `str.indexOf('\n---')` (lines 60, 93). This is a substring search, so
  `\n----` or `\n---x` closes too. The content starts after the delimiter, minus one
  `\r` and one `\n` (lines 116–122).
- **Unterminated:** `closeIndex = len` (line 95). The whole rest of the file is the
  matter, and the content is `''` (line 114).
- **Empty:** a block that is empty after removing `#` comment lines sets `isEmpty` and
  `data = {}` (lines 101–105).
- **BOM:** stripped (`lib/to-file.js` line 39 via `utils.toString`, `lib/utils.js` lines
  44–48).
- **Mapping:** not checked. The YAML engine is js-yaml `safeLoad` (`lib/engines.js`
  lines 15–18).

**`gray_matter` 0.3.2** (crate as published, commit 94967cb5cb), `Matter::parse` in
`src/matter.rs` lines 54–131:

- **Open:** the first line (up to the first `\n`) with `trim_end()` equals `---`
  (lines 81–86). `trim_end` removes Unicode whitespace, so trailing spaces, tabs and the
  CR of a CRLF are all allowed. A file without a `\n` has no front matter.
- **Close:** the next line whose `trim_end()` equals `---` or the configured close
  delimiter (line 93). `...` is not a close.
- **Unterminated:** from the code, `data` stays `None` and `content` is everything after
  the opening line (the opening line is dropped, lines 83 and 129).
- **Empty:** the trimmed matter is empty, so `data` stays `None` (lines 94–99).
- **BOM:** not handled.

## 5. python-frontmatter 1.3.0

- `parse` strips the whole text first (`__init__.py` line 75), so leading blank lines
  before `---` are accepted.
- **Open and close:** `YAMLHandler.FM_BOUNDARY = re.compile(r"^-{3,}\s*$", re.MULTILINE)`
  (`default_handlers.py` line 252). `detect` uses `.match`, which anchors at the start
  of the text (lines 196–207). `split` uses `FM_BOUNDARY.split(text, 2)`, so the
  opening line and the next boundary line close the block (lines 209–215). Three or
  more dashes count, and `...` does not.
- **Unterminated:** `split` yields too few parts and raises `ValueError`. `parse`
  catches it and returns no metadata and the whole text (lines 86–90).
- **Empty:** YAML loads `None`. Only a `dict` is merged (lines 93–95), so a non-mapping
  is ignored silently.
- **BOM:** the default encoding is `utf-8`, not `utf-8-sig`, and `str.strip` does not
  remove U+FEFF `[unverified]`.

## 6. Pandoc 3.13

The MANUAL (extension `yaml_metadata_block`) says: "A YAML metadata block is a valid YAML
object, delimited by a line of three hyphens (`---`) at the top and a line of three
hyphens (`---`) or three dots (`...`) at the bottom. The initial line `---` must not be
followed by a blank line. A YAML metadata block may occur anywhere in the document, but
if it is not at the beginning, it must be preceded by a blank line." It also says a
document may contain several blocks.

The code matches the manual:

- `yamlMetaBlock` (`src/Text/Pandoc/Readers/Metadata.hs` lines 162–178) reads
  `string "---"`, then `blankline` (spaces and tabs, then a newline:
  `Parsing/General.hs` lines 343–346), then `notFollowedBy blankline`, then lines until
  `stopLine`. `stopLine` is `---` or `...`, then `blankline` (lines 180–181).
- The block is one of the choices in `block` (`Readers/Markdown.hs` lines 486–490), so
  it is tried at every block start. This is the same reason pulldown accepts blocks
  mid-document.
- Before decoding, the lines are wrapped as `"---" : lines ++ ["..."]`, "so we allow yaml
  blocks with just comments" (Metadata.hs lines 171–172). `yamlBsToMeta` accepts an
  object, `[Null]` or no document, and fails otherwise with "expected YAML object"
  (lines 45–53). Because the parser is wrapped in `try`, a failing block is probably
  read as markdown instead `[design inference]`.
- **Empty:** `---\n---\n` passes the parser as written (the first content line is not
  blank, and `stopLine` matches at once) and gives empty metadata (from the code).
- **Unterminated:** `manyTill` reaches the end, `try` backtracks, and the lines are
  read as markdown.
- **BOM and CR:** `UTF8.toText` drops a leading BOM and filters CRs
  (`src/Text/Pandoc/UTF8.hs` lines 98–103). `readMarkdown` adds final newlines
  (`Readers/Markdown.hs` line 73), so a closing line at the end of the file still ends
  in a newline.

## 7. comrak 0.59.0

The option is `front_matter_delimiter` (no default; the user picks, for example `---`).
`Parser::parse` splits the front matter off before any block parsing
(`src/parser/mod.rs` lines 307–314) with `split_off_front_matter` (`src/strings.rs` lines
373–418):

- Leading BOMs are trimmed (line 374).
- **Open:** the text starts with the delimiter, then exactly LF or CRLF (lines 376–386).
  Trailing whitespace is not allowed.
- **Close:** search for `\n<delim>\r\n`, then `\n<delim>\n`, then `\n<delim>` (lines
  388–395). After the delimiter there must be the end of the text or a line break;
  otherwise the function returns `None` and **there is no front matter at all** (lines
  397–407). So `\n---x` before the real close cancels the whole block (from the code).
  One more blank line after the close is swallowed into the front matter
  (lines 409–415).
- **Empty:** `---\n---\n` is not front matter. The search needs a `\n` before the closing
  delimiter, and the opening line's `\n` has already been consumed (from the code, not
  run).
- **Unterminated:** no front matter.
- The content is kept raw as `NodeValue::FrontMatter` and not parsed.

## 8. markdown-rs 1.0.0 and micromark-extension-frontmatter 2.0.0

Both implement the same rule. Their docs say: "As there is no spec for frontmatter in
markdown, this extension follows how YAML frontmatter works on `github.com`." Whether
GitHub really behaves this way is `[unverified]`.

markdown-rs (`src/construct/frontmatter.rs`) gives the grammar on lines 8–20:

```
frontmatter ::= fence_open *( eol *byte ) eol fence_close
fence_open ::= sequence *space_or_tab
fence_close ::= sequence *space_or_tab
sequence ::= 3'+' | 3'-'
```

The same doc comment adds: "Frontmatter can only occur once. It cannot occur in a
container. It must have a closing fence. Like flow constructs, it must be followed by an
eol (line ending) or eof (end of file)."

- **Where:** only at the document start, after an optional BOM. `document::start` tries
  `BomStart` and then `before_frontmatter` (`src/construct/document.rs` lines 51–85).
- **Open:** exactly three markers (lines 96–116), then optional spaces and tabs, then a
  line ending. End of file right after the opening line is not allowed (lines 126–140).
- **Close:** exactly three of the same marker, optional spaces and tabs, then a line
  ending or end of file (lines 151–206). The closing fence is tried right after the
  opening line (lines 132–136), so an empty block is valid.
- **Unterminated:** reaching the end of file inside the content is `Nok` (lines
  255–260), so the text is read as ordinary markdown.
- **CRLF and CR:** the tokenizer treats CRLF and a lone CR as line endings
  (`src/tokenizer.rs` lines 785–795).
- No `...`. The content is not parsed.

micromark-extension-frontmatter (`dev/lib/syntax.js`) has the same states: `start`
requires column 1 and line 1 unless `anywhere` is set (lines 94–115). The open sequence
and whitespace are on lines 129–169, the line ending after the opening line on 183–197,
content on 211–265, and the closing fence on 287–388. The readme says: "using `anywhere`
is a terrible idea. It's called frontmatter, not matter-in-the-middle or so." micromark
skips a BOM at the start (`packages/micromark/dev/lib/preprocess.js` lines 50–54,
micromark 4.0.3).

## 9. markdown-it-front-matter 0.2.4

A block rule registered before `table` (`index.js` lines 130–142):

- **Open:** only at `startLine === 0` with `src.charCodeAt(0) === '-'` (line 24), and 3
  or more `-` (lines 30–41). The rest of the opening line is not checked.
- **Close:** a line of at least as many `-` as the opening line (lines 89–92), indented
  less than 4 (lines 78–81), with only spaces and tabs after it (lines 94–100). A line
  after a `...` line also ends the block (lines 60–62): `...` closes.
- **Unterminated:** "unclosed block should be autoclosed by end of document" (lines
  54–58), so the front matter runs to the end of the file.
- markdown-it turns `\r\n` and `\r` into `\n` before block rules
  (`lib/rules_core/normalize.mjs` lines 4–11, markdown-it 14.1.0).
- The raw text is passed to a callback (line 125) and not parsed.

## 10. Obsidian

Obsidian is closed source. The help page "Properties" says: "Properties are stored in
YAML format at the top of the file" and "Type `---` at the very beginning of a file". It
shows `---` above and below the properties. It does not mention `...`, trailing
whitespace, BOM, CRLF, unclosed blocks or empty blocks. All of those are `[unverified]`
for Obsidian. The page also says JSON may be used and "will be read, interpreted, and
saved as YAML".

## 11. YAML document markers and CommonMark

**YAML 1.2.2** (§9.1.2 Document Markers): `---` is the directives end marker
(`[203] c-directives-end ::= "---"`), and it also starts a document when no directives
precede it. `...` is the document end marker (`[204] c-document-end ::= "..."`), used
"to signal the parser to begin scanning for directives again". `[206] c-forbidden` is a
line that starts with either marker followed by a line break, white space or end of
input, and the spec says content lines are "forbidden to begin with either of these
markers". §9.1.4: an explicit document "may be completely empty"; Example 9.4 loads
`---`, `# Empty`, `...` as `null`. §9.2: after a document without `...`, the next
document must begin with `---`.

So `---`, YAML, `...` is one complete YAML document, which is why Jekyll, Pandoc and
pulldown accept `...` as the closing line. A `...` line inside a `---`…`---` block ends
the first YAML document. Text after it is a second document (saphyr returns two
documents for `id: a\n...\nx\n`; checked). A `---` line inside YAML would start another
document, which is why no frontmatter format can allow one in its content.

**CommonMark** has no frontmatter. The spec has no section on it `[gap]`. On the
CommonMark forum, John MacFarlane (spec author) wrote on 2016-09-23: "Front matter is
not going to be part of the spec." On 2016-09-27 he added: "If your application supports
front matter, it must strip this off, interpret it, and pass the rest to the CommonMark
parser." In CommonMark, a `---` line is a thematic break, or a setext underline after a
paragraph line.

## 12. Where the projects disagree

1. **Only at the start, or anywhere.** Pandoc and pulldown-cmark read a block at any
   block start, because the check sits in the per-block parser. micromark can do this
   with `anywhere`. Every other project reads one block at the start of the file.
   Jekyll's `\A`, comrak's `split_off_front_matter`, markdown-rs's
   `before_frontmatter` and markdown-it's `startLine === 0` make the start explicit.
2. **`...` as a close.** Yes: Jekyll, Pandoc, pulldown-cmark, markdown-it-front-matter
   (the YAML view: the block is a YAML document). No: Hugo, comrak, markdown-rs,
   micromark, both gray-matters, python-frontmatter (the fence view: the same line opens
   and closes). Obsidian: not documented.
3. **Unclosed block.** Read as markdown: Jekyll, pulldown-cmark, Pandoc, comrak,
   markdown-rs, micromark, python-frontmatter. Error: Hugo. The rest of the file
   becomes front matter: gray-matter (JS) and markdown-it-front-matter. The Rust
   `gray_matter` returns no data and drops the opening line. None of them reports a
   diagnostic and keeps going, as loci's `fm.unterminated` does.
4. **Empty block.** Accepted by nearly all. Not by pulldown-cmark (the first content line
   may not be a close or blank) and comrak (from the code).
5. **Fence length and shape.** Exactly three: pulldown, Jekyll, Pandoc, comrak,
   markdown-rs, micromark, `gray_matter`. Three or more: python-frontmatter,
   markdown-it-front-matter (the close at least as long as the open). Prefix match on
   the close: Hugo, gray-matter (JS).
6. **Trailing whitespace.** None: comrak. Spaces and tabs: Pandoc, markdown-rs,
   micromark, markdown-it-front-matter (close). Any whitespace: Jekyll,
   python-frontmatter, `gray_matter`. pulldown allows any ASCII whitespace on the open
   but only spaces on the close.
7. **Leading blank lines.** Hugo and python-frontmatter skip them before `---`; the others
   need `---` at byte 0 (after a BOM where they strip one).
8. **Content type.** Must be a mapping: Jekyll pages (error), Pandoc (object, null or
   nothing). Ignored when not a dict: python-frontmatter. Not parsed at all: comrak,
   markdown-rs, micromark, markdown-it-front-matter, pulldown-cmark.

## 13. Options for loci

**A. Keep pulldown's block at offset 0 plus patches (today).** Inherits `...`, the
space-only close, and "blank first line means no block", so `---\n\nid: a\n---\n` is
`fm.unterminated` today although it is closed (checked). It needs a separate read with
the option on, and the body read with it off.

**B. Split in loci, fence view (the proposal).** About the size of `empty_frontmatter`
plus the existing `fence`. The rule is stated in a few lines and needs no pulldown
quirks. pulldown's metadata option goes off everywhere, so the body is plain CommonMark
from byte 0, and `metadata_block` and `empty_frontmatter` go away.

**C. Split in loci, YAML view (also accept `...`).** Matches Jekyll and Pandoc, but the
user finds it odd, and only the projects that treat the block as a YAML document
accept it.

## Recommendation

Option B, with this exact rule:

- **BOM:** skip one U+FEFF at byte 0 (as today). Matches Jekyll, gray-matter (JS),
  Pandoc, comrak, markdown-rs and micromark.
- **Open:** the first line is `---`, then zero or more spaces and tabs, then LF or CRLF.
  Exactly three dashes, no indentation, at byte 0 (after the BOM); no leading blank
  lines.
- **Close:** the first later line that is `---`, then zero or more spaces and tabs, then
  LF, CRLF or the end of the file. The block's span includes that line break. `...` is
  content, not a close.
- **Line breaks:** LF and CRLF. A lone CR is not a line break (pulldown and markdown-rs
  treat it as one; such files are rare) `[design inference]`.
- **Content:** the bytes between the two lines are the YAML text. If saphyr finds no
  document (empty, blank or comment-only text), the frontmatter has no fields, as today.
  Otherwise it must be **exactly one** document whose top level is a mapping, else
  `fm.yaml`. The one-document check is new. Without it, a `...` line inside the block
  would end the YAML document and the text after it would be silently dropped (saphyr
  returns two documents; `parse_yaml` keeps the first).
- **Unterminated:** a first line that opens a block with no closing line is
  `fm.unterminated`. This includes a file that is only `---`. The span is the opening
  line, and the body is read from the start, as today. Failing loudly here matches
  Hugo's intent; every other project silently reads such a file as markdown.
- **Mid-document `---` blocks:** never frontmatter. With pulldown's metadata option off,
  `---`, text, `---` is a thematic break and a setext heading (or two breaks), as in
  CommonMark and as loci's body walk does today.

This is the markdown-rs / micromark rule (exactly three dashes, spaces and tabs, a close
followed by a line end or end of file, empty allowed, start of file only, no `...`).
loci differs from it in reporting an unclosed block instead of reading it as markdown,
and in parsing the YAML. On `...` it agrees with Hugo, comrak, both gray-matters and
python-frontmatter, and differs from Jekyll, Pandoc, pulldown-cmark and
markdown-it-front-matter. The opening and closing lines agree with Pandoc, apart from
`...`.

**Inputs that read differently from today.** "Today" is the current parser, run in a
copy of the repo; "proposed" is from the rule above.

| Input | Today | Proposed |
|---|---|---|
| `---\nid: a\n...\nbody\n` | fields `id`, closed by `...` | `fm.unterminated` |
| `---\nid: a\n...\nbody\n---\n` | fields `id`, block ends at `...`, body is `body`, `---` | one block to the last line; YAML is two documents, so `fm.yaml` |
| `---\n\nid: a\n---\nbody\n` | `fm.unterminated` | fields `id` |
| `---\nid: a\n---\t\nbody\n` | `fm.unterminated` | fields `id`; body `body` |
| `---\nid: a\n---\t\nbody\n---\n` | fields `id`, block runs to the last `---`, YAML text includes `---\t` and `body` (a second YAML document, ignored) | block closes at line 3; body is `body`, `---` |
| `---` + another ASCII whitespace, such as a form feed, then a newline | pulldown opens a block | not frontmatter (body from byte 0) `[design inference]` |

Unchanged (checked against today): CRLF files, a BOM, `--- ` with trailing spaces on
either line, a close at the end of the file without a newline, `---\n---\n`,
`---\n\n---\n`, a comment-only block (no fields), a block followed directly by another
`---` line, an indented ` ---` (not a close), `----` (not a close), and a non-mapping
block (`fm.yaml`).

The D244 rewrite rule still applies. When a rewrite empties the mapping, removing the
block could make a body that starts with `---` read as frontmatter, so loci must keep an
empty block in that case.

## Gaps

- Obsidian's rules beyond "`---` at the very beginning of a file" (closing line,
  `...`, whitespace, BOM, unclosed blocks): closed source and not documented
  `[unverified]`.
- GitHub's own frontmatter rendering, which markdown-rs and micromark say they follow:
  no primary source found `[unverified]`.
- Hugo: BOM handling outside `parser/pageparser`, and whether the front matter must be
  a map `[unverified]`. Its error on a file starting with `- item` comes from the code,
  not a run.
- comrak's rejection of `---\n---\n`, Pandoc's acceptance of it, and Pandoc reading a
  non-object block as markdown: from the code, not run.
- python-frontmatter and markdown-it-front-matter BOM handling `[unverified]`.

## Sources

pulldown-cmark 0.13.6 (crate source, commit 014b61d38d235f382935bb676bab872b7680afa2):
- `pulldown-cmark/src/firstpass.rs` lines 84, 319–331, 1421–1459:
  https://github.com/pulldown-cmark/pulldown-cmark/blob/014b61d38d235f382935bb676bab872b7680afa2/pulldown-cmark/src/firstpass.rs
- `pulldown-cmark/src/scanners.rs` lines 493–510, 533–552, 744–797:
  https://github.com/pulldown-cmark/pulldown-cmark/blob/014b61d38d235f382935bb676bab872b7680afa2/pulldown-cmark/src/scanners.rs

Jekyll v4.4.1 (1f319fb273):
- https://github.com/jekyll/jekyll/blob/v4.4.1/lib/jekyll/document.rb (lines 13, 75–80, 482–487)
- https://github.com/jekyll/jekyll/blob/v4.4.1/lib/jekyll/convertible.rb (lines 37–68)
- https://github.com/jekyll/jekyll/blob/v4.4.1/lib/jekyll/utils.rb (lines 141–145, 305–314)

Hugo v0.169.0 (1da10fc447):
- https://github.com/gohugoio/hugo/blob/v0.169.0/parser/pageparser/pagelexer_intro.go (lines 16–40, 122–159)
- https://github.com/gohugoio/hugo/blob/v0.169.0/parser/pageparser/pagelexer.go (`delimYAML` line 103; `consumeCRLF` lines 233–243)

gray-matter 4.0.3 (e54a33b394e14a1808b88f939507f374552906e4):
- https://github.com/jonschlinkert/gray-matter/blob/e54a33b394e14a1808b88f939507f374552906e4/index.js (lines 57–131, 206–219)
- `lib/to-file.js` (lines 38–39), `lib/utils.js` (lines 44–48, 63–66), `lib/engines.js` (lines 15–18), same commit

`gray_matter` 0.3.2 (crate source, commit 94967cb5cb5e5b1621328d055224537ee9dd8b69):
- https://github.com/the-alchemists-of-arland/gray-matter-rs/blob/94967cb5cb5e5b1621328d055224537ee9dd8b69/src/matter.rs (lines 54–131)

python-frontmatter v1.3.0 (dc7c0af546):
- https://github.com/eyeseast/python-frontmatter/blob/v1.3.0/frontmatter/default_handlers.py (lines 196–215, 246–260)
- https://github.com/eyeseast/python-frontmatter/blob/v1.3.0/frontmatter/__init__.py (lines 49–97)

Pandoc 3.13 (30b1354684):
- https://github.com/jgm/pandoc/blob/3.13/src/Text/Pandoc/Readers/Metadata.hs (lines 45–53, 162–181)
- https://github.com/jgm/pandoc/blob/3.13/src/Text/Pandoc/Readers/Markdown.hs (lines 71–73, 311–316, 486–490)
- https://github.com/jgm/pandoc/blob/3.13/src/Text/Pandoc/Parsing/General.hs (lines 339–346)
- https://github.com/jgm/pandoc/blob/3.13/src/Text/Pandoc/UTF8.hs (lines 98–103)
- https://github.com/jgm/pandoc/blob/3.13/MANUAL.txt (lines 5232–5267, "Extension: `yaml_metadata_block`")

comrak v0.59.0 (ac9d2081fe):
- https://github.com/kivikakk/comrak/blob/v0.59.0/src/strings.rs (lines 373–418)
- https://github.com/kivikakk/comrak/blob/v0.59.0/src/parser/mod.rs (lines 307–314, 359–367, 403–405)

markdown-rs 1.0.0:
- https://github.com/wooorm/markdown-rs/blob/1.0.0/src/construct/frontmatter.rs (lines 1–290)
- https://github.com/wooorm/markdown-rs/blob/1.0.0/src/construct/document.rs (lines 51–85)
- https://github.com/wooorm/markdown-rs/blob/1.0.0/src/tokenizer.rs (lines 785–795)

micromark-extension-frontmatter 2.0.0 and micromark 4.0.3:
- https://github.com/micromark/micromark-extension-frontmatter/blob/2.0.0/dev/lib/syntax.js (lines 94–388)
- https://github.com/micromark/micromark-extension-frontmatter/blob/2.0.0/readme.md (lines 47–50, 182–199)
- https://github.com/micromark/micromark/blob/4.0.3/packages/micromark/dev/lib/preprocess.js (lines 50–54)

markdown-it-front-matter v0.2.4 (a7f5e178f2) and markdown-it 14.1.0:
- https://github.com/ParkSB/markdown-it-front-matter/blob/v0.2.4/index.js (lines 10–142)
- https://github.com/markdown-it/markdown-it/blob/14.1.0/lib/rules_core/normalize.mjs (lines 4–17)

Obsidian: https://obsidian.md/help/properties (fetched 2026-11-04).

YAML 1.2.2: https://yaml.org/spec/1.2.2/ (§5.2, §9.1.2–9.1.4, §9.2; productions 203–206; Examples 9.4, 9.6).

CommonMark forum, "Front matter best practice?", replies by jgm on 2016-09-23 and
2016-09-27: https://talk.commonmark.org/t/front-matter-best-practice/2235

loci (working tree at 086eebc): `crates/loci-core/src/parse/frontmatter.rs` (lines
71–130, 336–350), `crates/loci-core/src/parse/mod.rs` (lines 243–251, 259–276),
`docs/specs/m0.md` §4.3, `docs/DECISIONS.md` D244–D245. Runs: throwaway tests in a copy of
the repo; inputs and outputs are as listed in §1 and the Recommendation.
