# Sift — learning plan

## Reference (north star)

**Primary source (read and re-read as you implement):**

- **Title:** [Fast regex search: indexing text for agent tools](https://cursor.com/blog/fast-regex-search)  
- **Author:** Vicent Marti  
- **Published:** 2026-03-23 (blog / research)  
- **URL:** https://cursor.com/blog/fast-regex-search  

Sift is an **independent, open-source learning project**. It is **not** Cursor’s product; it is your playground for the **same class of ideas**: narrow candidates with an index, then **verify** with the real regex. Use the article as the conceptual spine; your code will differ in details.

---

### Why this problem exists (intro)

- Agents still need **regex** for many tasks; semantic search does not replace it.
- **ripgrep** is excellent at *matching*, but it must still consider **all files** (or all candidates after ignore rules) — in huge monorepos that dominates latency.
- **Takeaway for Sift:** the index is not “faster regex”; it is **fewer places** to run regex. Verification stays mandatory.

---

### Article map — what each section is *for* (study notes)

Use this as a checklist while you read. “Try in Sift” = good exercise when you reach that phase.

| Section | Core idea | Try in Sift / note |
| --------|-----------|---------------------|
| **The classic algorithm** | N-gram / inverted-index ideas go back to Zobel, Moffat & Sacks-Davis (1993); Russ Cox popularized the trigram + regex story post–Google Code Search. | Phase 2–3: you’re in the same lineage as **codesearch**-style thinking. |
| **Inverted indexes** | Tokens → posting lists; **intersection** = AND; **union** where alternation means OR. | Phase 3: `intersect.rs`, plus tests for AND vs OR branches. |
| **Trigram decomposition** | Index: overlapping **3-grams** from text. Query: parse regex → extract grams from **literal** parts; classes/alternation complicate life. | Phase 2–3: `trigram.rs` + `planner.rs`; start with literals, add cases incrementally. |
| **Putting it all together** | Candidates from index are **possible** hits; only a full scan of those files with the regex gives **exact** matches. | Phase 1 vs 3: always keep a **naive baseline** to compare against. |
| **Trade-offs** | Simple query decomposition → few grams → huge candidate sets. Heavy decomposition → many posting list loads → can approach “scan everything.” | Phase 3–4: **gram choice** and **stats** (rare grams first) are the lesson. |
| **Suffix arrays (detour)** | **livegrep**-style: suffix array + binary search on literals; different trade-off (big concatenated corpus, harder incremental update). | Optional reading / spike — **not** required for your first working trigram index. |
| **Trigram + probabilistic masks** | “3.5-grams”: trigram keys + **extra bits** (e.g. next-char / position masks) to cut false *document* candidates; still probabilistic → verify. | Advanced Phase 4+ if you outgrow plain trigrams; watch **Bloom saturation**. |
| **Sparse n-grams** | Deterministic “weights” on character pairs → variable-length sparse grams; **index** pays more work, **query** uses a **covering** set; **frequency-weighted** pairs reduce lookups. | Advanced; Cursor/ClickHouse/GitHub code search territory — defer until classic trigrams work. |
| **All this, in your machine** | Local index: privacy, no round-trips, **fresh** enough for “agent read its own writes.” **Two files:** mmap’d sorted **hash → offset** table; contiguous **postings** blob read by offset. | Phase 4: mmap lexicon + postings layout; Phase 5 stays thin. |

**Related systems mentioned in the article (read source / docs when stuck):** [google/codesearch](https://github.com/google/codesearch), [sourcegraph/zoekt](https://github.com/sourcegraph/zoekt), [livegrep](https://livegrep.com/) / Nelson Elhage’s write-up linked from the post.

---

### One sentence to remember

**Index for recall, regex for precision** — the index shrinks the search space; the regex decides truth.

When in doubt about direction, re-read [the post](https://cursor.com/blog/fast-regex-search) and align your next phase (naive verify → persisted index → planner → performance) with that split.

---

## How this plan is meant to be used

- **You write the code.** This file is a **syllabus and architecture sketch**, not a spec for someone else to implement.
- **Follow the phases in order** unless you have a good reason to skip ahead (e.g. spike mmap in isolation). Later phases assume you understand the earlier ones.
- **Prefer small PRs to yourself:** one module or one behavior at a time, with tests before or right after the behavior.
- **It is fine to go slow.** Time estimates below are *rough* “if this were a full-time sprint” hints — for learning, multiply by what feels honest (often 2–10×).
- **Use the Cursor article as assigned reading** before each major phase; note what you’re implementing vs what you’re deferring (e.g. sparse n-grams can wait).

---

## Learning goals

By the end, you should be able to explain and implement:

1. **Why** naive regex over a whole tree breaks down on large repos (and where ripgrep still wins on *matching*, but not on *how many files* you touch).
2. **Trigram indexing** — build, serialize, load — and why verification is still required.
3. **Query planning** — turning a regex into a *small* set of grams to look up (heuristics and trade-offs).
4. **Intersection / unions** of posting lists and **correctness** vs the naive baseline.
5. **Performance ideas** — mmap, compact postings, frequency-aware gram choice, parallelism — as optional layers *after* correctness.

The CLI is practice in **keeping the core library honest**: if the API is awkward, fix the library, not the other way around.

---

## Target shape (what you’re building)

Build a **Rust library for indexed regex search over codebases** with a **thin CLI** that behaves like grep.

**Library first → CLI as a thin wrapper** so you learn:

- How to structure embeddable logic (agents, editors) vs I/O boundaries
- How to test the engine without the binary

### CLI & UX direction (ripgrep-shaped)

- **Primary interface matches ripgrep:** `sift [OPTIONS] PATTERN [PATH ...]` — same mental model as `rg` (pattern, paths, flags). Users should not *have* to run a separate `index` command for normal use.
- **Indexing is an implementation detail**, not the main UX: the **first search** (or a background step) **builds or refreshes** the on-disk index under a default or configured cache dir (e.g. `.index/` or `XDG_CACHE_HOME/...`). Staleness policy (mtimes, git revision, explicit `--rebuild`) is a later design choice.
- **Library API:** expose a **high-level** entry (e.g. `search` / `search_corpus`) that **ensures** the index exists and is usable before running the query pipeline. Keep **low-level** `build_index` / `Index::open` for tests, advanced embedders, and debugging — but they need not be the default **public** story if you prefer a minimal surface.
- **Phase 1 today** still uses explicit metadata + `Index::open`; migrating to “implicit index on first grep” is a **phase 5+ / UX** refactor once the index is real (Phase 2+).

---

## High-level architecture

```text
lib (core engine)     ← you implement almost everything here first
  ├── index building
  ├── storage (on-disk + mmap)
  ├── query planning
  ├── candidate retrieval
  ├── regex verification

cli (thin wrapper)    ← last: argument parsing + printing only
  ├── argument parsing
  ├── output formatting
  ├── calls into lib
```

**One line:** `core = search engine`, `cli = skin`.

---

## Crate structure (Rust workspace)

```text
sift/
  Cargo.toml          (workspace)

  crates/
    core/             # library (ALL logic lives here)
    cli/              # binary crate (thin wrapper)
```

You may start from an empty workspace, stubs, or a prior scaffold — what matters is that **behavior lives in `core`**.

---

## Library API (design target — keep it small)

Sketch this API early (even as `todo!()`), then fill it in phase by phase.

### Indexing (low-level; optional to expose publicly)

```rust
pub fn build_index(path: &Path, out_dir: &Path) -> Result<()>;
```

Prefer a **higher-level** API for embedders that hides cache layout and “first run” indexing (see “CLI & UX direction” above).

### Loading

```rust
pub struct Index;

impl Index {
    pub fn open(path: &Path) -> Result<Self>;
}
```

### Search

```rust
pub struct SearchMatchFlags: u8 { /* CASE_INSENSITIVE, INVERT_MATCH, … */ }

pub struct SearchOptions {
    pub flags: SearchMatchFlags,
    pub max_results: Option<usize>,
}

pub struct Match {
    pub file: PathBuf,
    pub line: usize,
    pub text: String,
}

impl Index {
    pub fn search(&self, patterns: &[String], opts: SearchOptions) -> Result<Vec<Match>>;
}
```

### Optional (strong for learning)

```rust
pub fn explain(&self, pattern: &str) -> QueryPlan;
```

`explain` forces you to make planning **inspectable** — useful when debugging “why did I scan so many files?”.

---

## Design rules (for your own reviews)

1. **CLI does nothing smart** — all logic stays in core.
2. **Library must be usable standalone** — e.g. `let index = Index::open(".index")?; let results = index.search("foo.*bar", opts)?;`
3. **No global state** — everything passed explicitly.
4. **Clear separation**

| Layer   | Responsibility        |
| ------- | --------------------- |
| index   | build + store         |
| planner | regex → grams         |
| query   | candidate retrieval   |
| verify  | exact matching        |
| cli     | UX only               |

---

## Roadmap (phases — you implement each)

### Phase 1 — Core library (naive search)

**Pacing hint:** on the order of days to a week of part-time work — depends on Rust comfort.

**Goal:** Understand end-to-end search **without** an index: walk, read lines, regex.

**Suggested modules / deps:**

```text
core/
  verify.rs    # regex (e.g. regex crate)
  search.rs    # orchestration: ignore::WalkBuilder + line scan + matches
```

Use the **`ignore`** crate for walking (ripgrep-class `.gitignore` / `.ignore` handling) — no custom `walker.rs`.

**Checkpoint (you decide you’re ready to move on):** You can search a directory tree and return `path:line:text` matches; you’re comfortable with UTF-8 edge cases you care about (e.g. lossy vs skip binary).

**Stretch:** `Index::search()` can call this path as a **fallback** forever — indexed search will narrow *candidates*, not replace verification.

---

### Phase 2 — Index builder (library)

**Pacing hint:** several days — serialization + debugging is where time goes.

**Goal:** Build a trigram-oriented index and **persist** it; load it back into memory.

**Suggested modules:**

```text
core/
  index/
    builder.rs
    trigram.rs
    files.rs
  storage/
    lexicon.rs
    postings.rs
```

**Responsibilities to implement yourself**

- **builder.rs:** Walk files, extract trigrams, assign file IDs, build in-memory structures.
- **trigram.rs:** e.g. `fn extract_trigrams(text: &str) -> ...` — start strict ASCII if you want, then widen.

**On-disk layout (example — you can adjust):**

```text
.index/
  files.bin
  lexicon.bin
  postings.bin
```

**Checkpoint:** `build_index()` writes files you can reload; `Index::open()` reconstructs enough state to *use* the index in the next phase (even if search still falls back to naive at first).

---

### Phase 3 — Indexed query engine

**Pacing hint:** several days — planner heuristics are easy to get wrong; lean on tests.

**Goal:** Use the index to **reduce** the set of files/lines you verify.

**Suggested modules:**

```text
core/
  planner.rs
  query.rs
  intersect.rs
```

**Responsibilities**

- **planner.rs:** Regex → literals / grams; pick a *small* set of lookup keys.
- **query.rs:** Load posting lists, intersect (and unions where needed).

**Search flow:**

```text
pattern
  → planner
  → trigrams (or grams)
  → postings
  → intersect
  → candidate files
  → verify regex
```

**Checkpoint:** Indexed path returns the same matches as naive on your test corpora; when the planner can’t narrow, you fall back safely.

---

### Phase 4 — Performance layer

**Pacing hint:** open-ended — treat as a menu, not a gate.

**Ideas to try (in any order)**

1. **mmap lexicon** — sorted hashes → binary search ([Cursor post](https://cursor.com/blog/fast-regex-search) describes the two-file layout idea at a high level).
2. **Compact postings** — `[offset, length]` into one blob file.
3. **Frequency stats** — prefer rarer grams at query time (pair-frequency tables show up in the sparse-ngram discussion in the article).
4. **Parallel verification** — e.g. `rayon::par_iter()` on candidate files.

**Checkpoint:** You can show a before/after on a non-trivial repo *you* choose, and explain *why* it got faster (fewer bytes read? fewer files? better gram choice?).

---

### Phase 5 — CLI wrapper

**Pacing hint:** a focused session or two once the library works.

**Goal:** Prove the API is usable from a binary: parse args, call `sift-core`, print grep-like lines.

**Layout:**

```text
cli/
  main.rs
```

**Example UX (target):**

```bash
# ripgrep-like — index build/refresh happens as needed
sift 'pattern' .
sift -i 'foo' src/

# optional advanced / debugging
sift --rebuild-index   # if you add an explicit refresh flag
```

(Legacy: explicit `sift index <corpus>` is fine during development; it should not be the primary story.)

**Output (grep-like):**

```text
path/file.rs:12: matched line
```

**Checkpoint:** No searching logic in `cli/` — only I/O and formatting.

---

## Testing strategy (your safety net)

Write tests **as you learn**, not only at the end.

- **Unit tests:** Trigram extraction, intersection, planner edge cases (alternation, classes).
- **Integration tests:** Small fixture dirs in `tests/` — build index → search → compare to naive.
- **Property / differential tests (optional but high value):** Indexed results == naive results for random patterns on a fixed corpus.

---

## Milestone (learning)

You can call the learning arc “done enough” when:

- You can **explain** the pipeline from the Cursor article in your own words, pointing to **your** modules.
- The library is **embeddable** (another crate could depend on `sift-core`).
- The CLI is a **thin** layer — grep-like output only.

**Demo target:**

```rust
let index = Index::open(".index")?;
let matches = index.search("class .*Service", opts)?;
```

**Why this structure matters:** Same engine can later plug into agents, editors, or daemons — you’re practicing separation of concerns, not shipping a product on day one.
