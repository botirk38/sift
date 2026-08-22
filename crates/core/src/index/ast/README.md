# index/ast/

AST index kind: a per-file language map and node-kind postings over the shared
snapshot `Files` id space, built on tree-sitter and `ast-grep-core`.

## Model

A structural match needs two things this kind can prove cheaply per file:

1. **Language** — the pattern only matches files its grammar can parse.
   `kinds.bin` maps every shared `FileId` to an [`AstLanguage`](language.rs)
   code (or none: unknown extension / non-UTF-8).
2. **Node kinds** — a compiled pattern advertises the node kinds a match can
   root at (`potential_kinds`). The kind directory maps `(lang, kind)` to a
   posting list of files whose parse tree contains that kind.

Both signals are necessary conditions, so queries may over-return but never
under-return; a query this kind cannot narrow receives every shared file id.
The structural query arm lands with the `--ast` search surface; until then
every query takes the full-id fallback.

## Languages

Seven languages, six grammar crates (`tree-sitter-typescript` ships both the
`typescript` and `tsx` grammar functions — they are distinct grammars with
distinct node-kind id spaces):

| Language | Code | Extensions |
|---|---|---|
| rust | 1 | rs |
| python | 2 | py py3 pyi bzl bazel |
| javascript | 3 | cjs js mjs jsx |
| typescript | 4 | ts cts mts |
| tsx | 5 | tsx |
| go | 6 | go |
| java | 7 | java |

Extension matching is case-sensitive (`Main.RS` maps to no language), matching
ast-grep 0.45.0; verification applies the same `from_path` rule, so the index
can only over-return. Codes are persisted and must never be renumbered. [`AstLanguage`](language.rs)
implements ast-grep's `Language`/`LanguageExt` directly over the pinned grammar
crates; the per-language settings (extension map, expando char, pattern
preprocessing) are vendored from `ast-grep-language` 0.45.0.

## On-disk format

Namespace `snapshots/<id>/ast/`; the shared `files.bin` stays at the snapshot
root.

| File | Magic | Layout |
|---|---|---|
| `kinds.bin` | `SIFTAKN1` | `magic(8) │ lang_count u32 │ file_count u32 │ lang_table[lang_count] { code u16 │ pad u16 │ grammar_fp u64 } │ file_lang[file_count] u16 (0xFFFF = none) │ dir_count u32 │ directory[dir_count] { lang u16 │ kind u16 │ offset u64 │ len u32 }` sorted by `(lang, kind)` |
| `postings.bin` | `SIFTPST3` | the shared `Postings` container (`index/postings.rs`); directory offsets address its payload |

Open-time validation: magics, section lengths, strictly increasing language
codes, file→language references, strictly sorted directory, contiguous
in-bounds posting ranges, every list decodes with ids inside the shared space,
no trailing bytes. After validation, `.expect("validated at open")` is the
accepted idiom.

`grammar_fp` fingerprints the compiled grammar (ABI version, node kinds and
named flags, field names). A mismatch marks that language **stale**: its kind
postings key a different grammar build, so it stops narrowing (files stay
covered) — never an open error, because `sift index update` must be able to
rebuild the store through `Indexes::open`.

## Build

`Index::build(dir, &Files)` parses every shared file (rayon per file, one
`tree_sitter::Parser` per task), records the language and the distinct node
kinds — named and unnamed, since matchers can root at either — then packs
`(lang, kind, file)` keys, sorts, and group-encodes posting lists. Non-UTF-8
files get no language: verification applies the identical rule, so excluding
them cannot under-return. Artifacts are always written, including for an
empty corpus — a manifest-listed namespace with missing artifacts would make
the whole snapshot unopenable.

There is no incremental update state; `Indexes::build()` rebuilds every kind.
