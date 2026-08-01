#![no_main]

//! Fuzzes the posting-list decoder on untrusted bytes.
//!
//! A real index is built once, then each iteration overwrites `postings.bin`
//! with a valid container header wrapping arbitrary payload bytes and reopens
//! the index. Opening runs the lexicon/postings integrity check, which decodes
//! every posting list, so this exercises `Postings::decode_sorted` (block
//! headers, `num_bits`, bitpacked blocks, and the delta-varint tail) against
//! adversarial input. Decoding must always return an error rather than panic,
//! over-allocate, or read out of bounds.

use libfuzzer_sys::fuzz_target;
use sift_core::grep::VisibilityConfig;
use sift_core::{
    CorpusKind, CorpusMeta, CorpusSpec, FilterMeta, GramWidth, IndexConfig, IndexCoverage,
    IndexRecord, IndexWalkConfig, Indexes, NGramIndex, StoreMeta, WalkMeta,
};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::{fs, io::Write};

struct Harness {
    _temp: tempfile::TempDir,
    sift_dir: PathBuf,
    postings: PathBuf,
    header: Vec<u8>,
    meta: StoreMeta,
}

static HARNESS: OnceLock<Harness> = OnceLock::new();

fn harness() -> &'static Harness {
    HARNESS.get_or_init(|| {
        let tmp = tempfile::tempdir().expect("tempdir");
        let corpus = tmp.path().join("c");
        fs::create_dir_all(&corpus).expect("mkdir");
        fs::write(corpus.join("a.txt"), b"hello world\nfoo bar\n").expect("a.txt");
        fs::write(corpus.join("b.txt"), b"baz\nquux line\n").expect("b.txt");
        let sift_dir = tmp.path().join(".sift");
        let trigram_dir = sift_dir.join("trigram");
        let config = IndexConfig {
            corpus: CorpusSpec {
                root: &corpus,
                kind: CorpusKind::Directory,
                follow_links: false,
                include_paths: &[],
                exclude_paths: &[],
            },
            walk: IndexWalkConfig::new(false),
            visibility: VisibilityConfig::default(),
        };
        NGramIndex::new()
            .width(GramWidth::TRIGRAM)
            .build(sift_core::IndexWrite {
                dest: sift_core::IndexDestination::Directory(&trigram_dir),
                config: &config,
                paths: &[],
            })
            .expect("build_index");
        let postings = trigram_dir.join("postings.bin");
        let magic = fs::read(&postings).expect("read postings")[..8].to_vec();
        let meta = StoreMeta::new(
            CorpusMeta {
                root: corpus.clone(),
                kind: CorpusKind::Directory,
                include_paths: Vec::new(),
                exclude_paths: Vec::new(),
            },
            IndexCoverage::Complete,
            WalkMeta {
                follow_links: false,
                one_file_system: false,
                max_depth: None,
                max_filesize: None,
            },
            FilterMeta {
                visibility: VisibilityConfig::default(),
            },
            vec![IndexRecord::ngram(GramWidth::TRIGRAM)],
        );
        Harness {
            _temp: tmp,
            sift_dir,
            postings,
            header: magic,
            meta,
        }
    })
}

fuzz_target!(|data: &[u8]| {
    let h = harness();
    let Ok(len) = u32::try_from(data.len()) else {
        return;
    };
    let mut blob = Vec::with_capacity(h.header.len() + 4 + data.len());
    blob.extend_from_slice(&h.header);
    blob.extend_from_slice(&len.to_le_bytes());
    blob.extend_from_slice(data);

    let Ok(mut file) = fs::File::create(&h.postings) else {
        return;
    };
    if file.write_all(&blob).is_err() {
        return;
    }
    drop(file);

    let _ = Indexes::open(&h.sift_dir, &h.meta);
});
