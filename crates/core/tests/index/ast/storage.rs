use std::fs;
use std::path::{Path, PathBuf};

use sift_core::{AstIndex, AstLanguage, Files, IndexRecord};
use tempfile::TempDir;

use crate::common::sample_store_meta;

const KINDS_MAGIC_LEN: usize = 8;

fn built_ast_dir(tmp: &TempDir) -> (PathBuf, usize) {
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");
    fs::write(corpus.join("main.rs"), "fn main() {}\n").expect("write rs");
    fs::write(corpus.join("app.py"), "def run():\n    return 1\n").expect("write py");

    let root = corpus.canonicalize().expect("canonicalize");
    let meta = sample_store_meta(root, vec![IndexRecord::Ast]);
    let snapshot = tmp.path().join("snapshot");
    fs::create_dir_all(&snapshot).expect("snapshot dir");
    let files = Files::build(&meta, &snapshot).expect("files");
    let ast_dir = tmp.path().join("ast");
    AstIndex::build(&ast_dir, &files).expect("build");
    (ast_dir, files.len())
}

fn artifact_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).expect("read artifact")
}

fn rewrite(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).expect("write artifact");
}

#[test]
fn open_rejects_missing_components() {
    let tmp = TempDir::new().expect("tempdir");
    let (ast_dir, file_count) = built_ast_dir(&tmp);
    fs::remove_file(ast_dir.join("kinds.bin")).expect("remove");
    assert!(AstIndex::open(&ast_dir, file_count).is_err());

    let tmp = TempDir::new().expect("tempdir");
    let (ast_dir, file_count) = built_ast_dir(&tmp);
    fs::remove_file(ast_dir.join("postings.bin")).expect("remove");
    assert!(AstIndex::open(&ast_dir, file_count).is_err());
}

#[test]
fn open_rejects_bad_magic() {
    let tmp = TempDir::new().expect("tempdir");
    let (ast_dir, file_count) = built_ast_dir(&tmp);
    let kinds = ast_dir.join("kinds.bin");
    let mut bytes = artifact_bytes(&kinds);
    bytes[0] = b'X';
    rewrite(&kinds, &bytes);
    assert!(AstIndex::open(&ast_dir, file_count).is_err());
}

#[test]
fn open_rejects_truncated_table() {
    let tmp = TempDir::new().expect("tempdir");
    let (ast_dir, file_count) = built_ast_dir(&tmp);
    let kinds = ast_dir.join("kinds.bin");
    let mut bytes = artifact_bytes(&kinds);
    bytes.truncate(KINDS_MAGIC_LEN + 6);
    rewrite(&kinds, &bytes);
    assert!(AstIndex::open(&ast_dir, file_count).is_err());
}

#[test]
fn open_rejects_trailing_bytes() {
    let tmp = TempDir::new().expect("tempdir");
    let (ast_dir, file_count) = built_ast_dir(&tmp);
    let kinds = ast_dir.join("kinds.bin");
    let mut bytes = artifact_bytes(&kinds);
    bytes.extend_from_slice(b"TRAILING");
    rewrite(&kinds, &bytes);
    assert!(AstIndex::open(&ast_dir, file_count).is_err());
}

#[test]
fn open_rejects_file_count_mismatch() {
    let tmp = TempDir::new().expect("tempdir");
    let (ast_dir, file_count) = built_ast_dir(&tmp);
    assert!(AstIndex::open(&ast_dir, file_count + 1).is_err());
}

#[test]
fn open_rejects_directory_offsets_past_payload() {
    let tmp = TempDir::new().expect("tempdir");
    let (ast_dir, file_count) = built_ast_dir(&tmp);
    // Grow the first directory offset far past the postings payload.
    let kinds = ast_dir.join("kinds.bin");
    let mut bytes = artifact_bytes(&kinds);
    let lang_count = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes")) as usize;
    let files = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes")) as usize;
    let dir_start = 16 + lang_count * 12 + files * 2 + 4;
    // The offset u64 sits after { lang u16 | kind u16 } in the entry.
    let off = dir_start + 4;
    bytes[off..off + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    rewrite(&kinds, &bytes);
    assert!(AstIndex::open(&ast_dir, file_count).is_err());
}

#[test]
fn grammar_fingerprint_mismatch_marks_language_stale() {
    let tmp = TempDir::new().expect("tempdir");
    let (ast_dir, file_count) = built_ast_dir(&tmp);
    // Flip a byte of the first language table entry fingerprint (the entry
    // starts at 16; the fingerprint u64 sits after { code u16 | pad u16 }).
    let kinds = ast_dir.join("kinds.bin");
    let mut bytes = artifact_bytes(&kinds);
    bytes[20] ^= 0xFF;
    rewrite(&kinds, &bytes);
    let index = AstIndex::open(&ast_dir, file_count).expect("open despite stale grammar");
    assert_eq!(index.stale_languages(), vec![AstLanguage::Rust]);
    // A stale language never narrows, but open itself must not fail:
    // `sift index update` has to be able to rebuild the store.
    assert_eq!(index.files_of(AstLanguage::Python).len(), 1);
}
