//! Per-file parse and kind-posting assembly for the AST kind.

use rustc_hash::FxHashSet;

use super::language::AstLanguage;
use super::storage::kinds::NO_LANGUAGE;
use crate::index::Files;
use crate::index::postings::Postings;

/// One kind directory row: `(lang, kind)` and its posting-list range in the
/// sibling `postings.bin` payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirEntry {
    pub lang: u16,
    pub kind: u16,
    pub offset: u64,
    pub len: u32,
}

/// Collected AST index data ready for persistence.
pub struct KindTables {
    pub file_lang: Vec<u16>,
    pub directory: Vec<DirEntry>,
    pub postings: Vec<u8>,
}

impl KindTables {
    /// Parse every shared file and assemble the language map and kind
    /// postings. Always yields tables, including for an empty corpus.
    ///
    /// # Errors
    ///
    /// Returns an error if reading a corpus file fails or a table dimension
    /// overflows its on-disk width.
    pub fn assemble(files: &Files) -> crate::Result<Self> {
        use rayon::prelude::*;

        if files.len() > u32::MAX as usize {
            return Err(crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "too many indexed files",
            )));
        }

        let root = files.root();
        let per_file = (0..files.len())
            .into_par_iter()
            .map(|id| {
                let rel = files
                    .rel_path(crate::index::FileId::new(id))
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("missing path for file id {id}"),
                        )
                    })?;
                let Some(lang) = AstLanguage::from_path(std::path::Path::new(rel)) else {
                    return Ok((NO_LANGUAGE, Vec::new()));
                };
                let bytes = std::fs::read(root.join(rel))?;
                if std::str::from_utf8(&bytes).is_err() {
                    // Verification applies the identical rule: non-UTF-8 files
                    // never match structurally, so excluding them here cannot
                    // under-return.
                    return Ok((NO_LANGUAGE, Vec::new()));
                }
                Ok((lang.code(), Self::kind_set(lang, &bytes)))
            })
            .collect::<std::io::Result<Vec<(u16, Vec<u16>)>>>()
            .map_err(crate::Error::Io)?;

        let file_lang: Vec<u16> = per_file.iter().map(|(lang, _)| *lang).collect();

        let mut keys = Vec::new();
        for (id, (lang, kinds)) in per_file.iter().enumerate() {
            if *lang == NO_LANGUAGE {
                continue;
            }
            let fid = u32::try_from(id).expect("file count checked above");
            for &kind in kinds {
                keys.push((u64::from(*lang) << 48) | (u64::from(kind) << 32) | u64::from(fid));
            }
        }
        rayon::slice::ParallelSliceMut::par_sort_unstable(&mut keys[..]);

        let mut postings = Vec::with_capacity(keys.len());
        let mut directory = Vec::new();
        let mut ids = Vec::new();
        let mut i = 0;
        while i < keys.len() {
            let group = keys[i] >> 32;
            let start = i;
            while i < keys.len() && keys[i] >> 32 == group {
                i += 1;
            }
            let offset = u64::try_from(postings.len()).map_err(|_| {
                crate::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "postings offset overflow",
                ))
            })?;
            ids.clear();
            ids.extend(keys[start..i].iter().map(|key| {
                u32::try_from(*key & u64::from(u32::MAX)).expect("masked file id fits in u32")
            }));
            let encoded = Postings::encode_list(&ids);
            let len = u32::try_from(encoded.len()).map_err(|_| {
                crate::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "posting list too long",
                ))
            })?;
            postings.extend_from_slice(&encoded);
            directory.push(DirEntry {
                lang: u16::try_from(group >> 16).expect("language code fits in u16"),
                kind: u16::try_from(group & u64::from(u16::MAX)).expect("kind id fits in u16"),
                offset,
                len,
            });
        }

        Ok(Self {
            file_lang,
            directory,
            postings,
        })
    }

    /// Distinct node-kind ids in `bytes` parsed as `lang`, named and unnamed
    /// alike (ast-grep matchers can root at either).
    fn kind_set(lang: AstLanguage, bytes: &[u8]) -> Vec<u16> {
        let mut parser = tree_sitter::Parser::new();
        let grammar = lang.ts_language();
        let mut kinds = FxHashSet::default();
        let parsed = parser
            .set_language(&grammar)
            .is_ok()
            .then(|| parser.parse(bytes, None))
            .flatten();
        if let Some(tree) = parsed {
            let mut cursor = tree.walk();
            'walk: loop {
                kinds.insert(cursor.node().kind_id());
                if cursor.goto_first_child() {
                    continue;
                }
                loop {
                    if cursor.goto_next_sibling() {
                        break;
                    }
                    if !cursor.goto_parent() {
                        break 'walk;
                    }
                }
            }
        } else {
            // tree-sitter returns no tree only when the grammar cannot be set
            // or parsing is cancelled; neither happens here. If it ever does,
            // claim every kind so narrowing can only over-return.
            let count = u16::try_from(grammar.node_kind_count()).unwrap_or(u16::MAX);
            kinds.extend(0..count);
        }
        let mut kinds: Vec<u16> = kinds.into_iter().collect();
        kinds.sort_unstable();
        kinds
    }
}
