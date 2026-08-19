#![no_main]

//! Fuzzes the AST kind-table decoder on untrusted bytes.
//!
//! A real AST index is built once, then each iteration overwrites `kinds.bin`
//! with the valid magic followed by arbitrary bytes and reopens the index.
//! Opening validates the language table, per-file language map, kind
//! directory, and every referenced posting list, so this exercises the whole
//! `kinds.bin` validation surface against adversarial input. Decoding must
//! always return an error rather than panic, over-allocate, or read out of
//! bounds.

use libfuzzer_sys::fuzz_target;
use sift_core::{
    AstIndex, CorpusMeta, Files, FilterMeta, IndexCoverage, IndexRecord, StoreMeta,
    VisibilityConfig, WalkMeta,
};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::{fs, io::Write};

struct Harness {
    _temp: tempfile::TempDir,
    ast_dir: PathBuf,
    kinds: PathBuf,
    file_count: usize,
}

static HARNESS: OnceLock<Harness> = OnceLock::new();

fn harness() -> &'static Harness {
    HARNESS.get_or_init(|| {
        let tmp = tempfile::tempdir().expect("tempdir");
        let corpus = tmp.path().join("c");
        fs::create_dir_all(&corpus).expect("mkdir");
        fs::write(corpus.join("a.rs"), b"fn main() { let x = 1; }\n").expect("a.rs");
        fs::write(corpus.join("b.py"), b"def run():\n    return 1\n").expect("b.py");
        let ast_dir = tmp.path().join("ast");
        let meta = StoreMeta::new(
            CorpusMeta {
                root: corpus.clone(),
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
            vec![IndexRecord::Ast],
        );
        let snapshot = tmp.path().join("snapshot");
        fs::create_dir(&snapshot).expect("snapshot");
        let files = Files::build(&meta, &snapshot).expect("files");
        let file_count = files.len();
        AstIndex::build(&ast_dir, &files).expect("build_index");
        let kinds = ast_dir.join("kinds.bin");
        Harness {
            _temp: tmp,
            ast_dir,
            kinds,
            file_count,
        }
    })
}

fuzz_target!(|data: &[u8]| {
    let h = harness();
    let mut blob = Vec::with_capacity(8 + data.len());
    blob.extend_from_slice(b"SIFTAKN1");
    blob.extend_from_slice(data);

    let Ok(mut file) = fs::File::create(&h.kinds) else {
        return;
    };
    if file.write_all(&blob).is_err() {
        return;
    }
    drop(file);

    // Reopen the kind tables from the corrupted blob.
    let _ = AstIndex::open(&h.ast_dir, h.file_count);
});
