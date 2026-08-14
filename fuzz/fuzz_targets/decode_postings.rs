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
use sift_core::{
    CorpusKind, CorpusMeta, Files, FilterMeta, GramNorm, GramWidth, IndexCoverage, IndexRecord,
    NGramIndex, StoreMeta, VisibilityConfig, WalkMeta,
};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::{fs, io::Write};

struct Harness {
    _temp: tempfile::TempDir,
    trigram_dir: PathBuf,
    postings: PathBuf,
    header: Vec<u8>,
    file_count: usize,
}

static HARNESS: OnceLock<Harness> = OnceLock::new();

fn harness() -> &'static Harness {
    HARNESS.get_or_init(|| {
        let tmp = tempfile::tempdir().expect("tempdir");
        let corpus = tmp.path().join("c");
        fs::create_dir_all(&corpus).expect("mkdir");
        fs::write(corpus.join("a.txt"), b"hello world\nfoo bar\n").expect("a.txt");
        fs::write(corpus.join("b.txt"), b"baz\nquux line\n").expect("b.txt");
        let trigram_dir = tmp.path().join("trigram");
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
        let files = Files::build(&meta).expect("files");
        let file_count = files.len();
        NGramIndex::build(GramWidth::TRIGRAM, GramNorm::Identity, &trigram_dir, &files)
            .expect("build_index");
        let postings = trigram_dir.join("postings.bin");
        let magic = fs::read(&postings).expect("read postings")[..8].to_vec();
        Harness {
            _temp: tmp,
            trigram_dir,
            postings,
            header: magic,
            file_count,
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

    // Reopen the kind tables against the corrupted postings blob.
    let _ = NGramIndex::open(
        GramWidth::TRIGRAM,
        GramNorm::Identity,
        &h.trigram_dir,
        h.file_count,
    );
});
