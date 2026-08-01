mod build;
pub(crate) mod files;
pub mod gram;
pub mod storage;

mod candidates;
mod index;
mod literals;

pub use gram::{Gram, GramNorm, GramWidth, GramWindows};
pub use index::{Index, NGramIndexError};

#[cfg(test)]
mod candidate_tests {
    use std::path::Path;

    use crate::index::ngram::storage::postings::Postings;
    use crate::search::{
        CaseMode, InputEncoding, SearchFlags, SearchOptions, SearchQuery, SearchQueryBuilder,
    };

    use super::*;

    fn default_config() -> Index {
        Index::new().width(GramWidth::TRIGRAM)
    }

    fn regex_search_options(
        case_insensitive: bool,
        word_regexp: bool,
        line_regexp: bool,
    ) -> SearchOptions {
        let mut flags = SearchFlags::empty();
        if word_regexp {
            flags |= SearchFlags::WORD_REGEXP;
        }
        if line_regexp {
            flags |= SearchFlags::LINE_REGEXP;
        }
        SearchOptions {
            flags,
            case_mode: if case_insensitive {
                CaseMode::Insensitive
            } else {
                CaseMode::Sensitive
            },
            input_encoding: InputEncoding::Raw,
            ..SearchOptions::default()
        }
    }

    fn built_query(patterns: &[String], options: SearchOptions) -> SearchQuery {
        SearchQueryBuilder::new(patterns.to_vec())
            .options(options)
            .build()
            .expect("query")
    }

    fn extracts_literals(
        patterns: &[String],
        case_insensitive: bool,
        word_regexp: bool,
        line_regexp: bool,
    ) -> bool {
        let query = built_query(
            patterns,
            regex_search_options(case_insensitive, word_regexp, line_regexp),
        );
        default_config().extract_literal_arms(&query).is_some()
    }

    fn full_scan(
        patterns: &[String],
        case_insensitive: bool,
        word_regexp: bool,
        line_regexp: bool,
    ) -> bool {
        let query = built_query(
            patterns,
            regex_search_options(case_insensitive, word_regexp, line_regexp),
        );
        default_config().extract_literal_arms(&query).is_none()
    }

    #[test]
    fn merge_sorted_runs_preserves_order_and_uniqueness() {
        let merged = Index::merge_sorted_runs(vec![vec![1, 3, 7], vec![1, 2, 7, 9], vec![4, 7, 8]]);
        assert_eq!(merged, vec![1, 2, 3, 4, 7, 8, 9]);
    }

    #[test]
    fn intersect_sorted_posting_byte_slices_handles_smallest_first_order() {
        let a = Postings::encode_list(&[1, 3, 5, 7, 9]);
        let b = Postings::encode_list(&[3, 7]);
        let c = Postings::encode_list(&[0, 3, 4, 7, 8]);
        let slices = vec![a.as_slice(), b.as_slice(), c.as_slice()];
        let ids = Index::intersect_sorted_slices(&slices);
        assert_eq!(ids, vec![3, 7]);
    }

    #[test]
    fn merge_sorted_runs_empty_input_returns_empty() {
        let merged = Index::merge_sorted_runs(vec![]);
        assert!(merged.is_empty());
    }

    #[test]
    fn merge_sorted_runs_single_list_returns_as_is() {
        let merged = Index::merge_sorted_runs(vec![vec![1, 2, 3]]);
        assert_eq!(merged, vec![1, 2, 3]);
    }

    #[test]
    fn merge_sorted_runs_with_empty_lists_mixed_in() {
        let merged = Index::merge_sorted_runs(vec![vec![1, 3], vec![], vec![2, 3]]);
        assert_eq!(merged, vec![1, 2, 3]);
    }

    #[test]
    fn intersect_sorted_posting_byte_slices_empty_input_returns_empty() {
        let ids = Index::intersect_sorted_slices(&[]);
        assert!(ids.is_empty());
    }

    #[test]
    fn intersect_sorted_slices_single_returns_decoded_ids() {
        let a = Postings::encode_list(&[1, 3, 5]);
        let ids = Index::intersect_sorted_slices(&[a.as_slice()]);
        assert_eq!(ids, vec![1, 3, 5]);
    }

    #[test]
    #[should_panic(expected = "postings validated at open")]
    fn intersect_sorted_slices_invalid_varint_panics() {
        let a = &[0xff];
        Index::intersect_sorted_slices(&[a]);
    }

    #[test]
    fn intersect_sorted_slices_no_overlap_returns_empty() {
        let a = Postings::encode_list(&[1, 2, 3]);
        let b = Postings::encode_list(&[4, 5, 6]);
        let ids = Index::intersect_sorted_slices(&[a.as_slice(), b.as_slice()]);
        assert!(ids.is_empty());
    }

    #[test]
    fn literal_narrows() {
        assert!(extracts_literals(
            &["beta".to_string()],
            false,
            false,
            false
        ));
    }

    #[test]
    fn dot_star_full_scan() {
        assert!(full_scan(&[".*".to_string()], false, false, false));
    }

    #[test]
    fn alternation_narrows() {
        assert!(extracts_literals(
            &[r"foo|bar".to_string()],
            false,
            false,
            false
        ));
    }

    #[test]
    fn word_literal_narrows() {
        assert!(extracts_literals(&["beta".to_string()], false, true, false));
    }

    #[test]
    fn line_regexp_narrows() {
        assert!(extracts_literals(&["beta".to_string()], false, false, true));
    }

    #[test]
    fn case_insensitive_narrows() {
        assert!(extracts_literals(&["beta".to_string()], true, false, false));
    }

    #[test]
    fn case_insensitive_alternation_keeps_long_arms() {
        let patterns = ["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string()];
        let built = built_query(&patterns, regex_search_options(true, false, false));
        let arms = default_config()
            .extract_literal_arms(&built)
            .expect("casei alternation should extract");
        assert_eq!(arms.len(), 4);
        assert!(arms.iter().all(|arm| arm.len() >= 7));
        assert!(arms.iter().any(|arm| arm == b"ERR_SYS"));
    }

    #[test]
    fn case_insensitive_fixed_string_keeps_original_bytes() {
        let patterns = ["ERR_SYS".to_string()];
        let options = SearchOptions {
            flags: SearchFlags::FIXED_STRINGS,
            case_mode: CaseMode::Insensitive,
            input_encoding: InputEncoding::Raw,
            ..SearchOptions::default()
        };
        let built = built_query(&patterns, options);
        let arms = default_config()
            .extract_literal_arms(&built)
            .expect("fixed casei should extract");
        assert_eq!(arms, vec![b"ERR_SYS".to_vec()]);
    }

    #[test]
    fn case_insensitive_non_ascii_declines_narrowing() {
        let patterns = ["café".to_string()];
        let options = SearchOptions {
            flags: SearchFlags::FIXED_STRINGS,
            case_mode: CaseMode::Insensitive,
            input_encoding: InputEncoding::Raw,
            ..SearchOptions::default()
        };
        let built = built_query(&patterns, options);
        assert!(default_config().extract_literal_arms(&built).is_none());
    }

    #[test]
    fn required_literal_inside_regex_narrows() {
        assert!(extracts_literals(
            &["[A-Z]+_RESUME".to_string()],
            false,
            false,
            false
        ));
    }

    #[test]
    fn unicode_class_full_scan() {
        assert!(full_scan(&[r"\p{Greek}".to_string()], false, false, false));
    }

    #[test]
    fn no_literal_full_scan() {
        assert!(full_scan(
            &[r"\w{5}\s+\w{5}".to_string()],
            false,
            false,
            false
        ));
    }

    #[test]
    fn short_literal_covers_with_wildcard_grams() {
        assert!(extracts_literals(&["ab".to_string()], false, false, false));
    }

    #[test]
    fn generic_width_uses_spec_width_for_literal_extraction() {
        let patterns = ["ab".to_string()];
        let built = built_query(
            &patterns,
            SearchOptions {
                input_encoding: InputEncoding::Raw,
                ..SearchOptions::default()
            },
        );
        assert!(
            Index::new()
                .width(GramWidth::new(2))
                .extract_literal_arms(&built)
                .is_some()
        );
    }

    #[test]
    fn fixed_string_narrows() {
        let patterns = ["beta.gamma".to_string()];
        let options = SearchOptions {
            flags: SearchFlags::FIXED_STRINGS,
            input_encoding: InputEncoding::Raw,
            ..SearchOptions::default()
        };
        let built = built_query(&patterns, options);
        assert!(default_config().extract_literal_arms(&built).is_some());
    }

    #[test]
    fn open_tables_accepts_count_mismatch() {
        use crate::index::ngram::storage::format::{
            FILES_MAGIC, GRAMS_MAGIC, LEXICON_MAGIC, POSTINGS_MAGIC,
        };
        use tempfile::TempDir;

        let tmp = TempDir::new().expect("create temp dir");
        let dir = tmp.path().join("index");
        std::fs::create_dir(&dir).expect("create index dir");

        let mut files = FILES_MAGIC.to_vec();
        files.extend_from_slice(&0u32.to_le_bytes());
        std::fs::write(dir.join("files.bin"), &files).expect("write files");

        let mut lex = LEXICON_MAGIC.to_vec();
        lex.extend_from_slice(&3u32.to_le_bytes());
        lex.extend_from_slice(&1u32.to_le_bytes());
        lex.extend_from_slice(&0u32.to_le_bytes());
        lex.extend_from_slice(b"abc");
        lex.extend_from_slice(&0u64.to_le_bytes());
        lex.extend_from_slice(&3u32.to_le_bytes());
        std::fs::write(dir.join("lexicon.bin"), &lex).expect("write lexicon");

        let posting_payload = Postings::encode_list(&[0, 1]);
        let mut pb = POSTINGS_MAGIC.to_vec();
        pb.extend_from_slice(&u32::try_from(posting_payload.len()).unwrap().to_le_bytes());
        pb.extend_from_slice(&posting_payload);
        std::fs::write(dir.join("postings.bin"), &pb).expect("write postings");

        let mut grams = GRAMS_MAGIC.to_vec();
        grams.extend_from_slice(&3u32.to_le_bytes());
        grams.extend_from_slice(&0u32.to_le_bytes());
        grams.extend_from_slice(&0u32.to_le_bytes());
        std::fs::write(dir.join(crate::GRAMS_BIN), &grams).expect("write grams");

        // Posting count mismatches are caught at build time.
        // The open path skips content-level validation for speed.
        let result = Index::new().width(GramWidth::TRIGRAM).open_from(
            &dir,
            Path::new("/root"),
            crate::index::CorpusKind::Directory,
        );
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod persistence_tests {
    use std::path::PathBuf;

    use super::files::FileFingerprint;
    use super::*;

    #[test]
    fn validate_file_paths_accepts_normal_relative_paths() {
        let fps = vec![
            FileFingerprint {
                path: PathBuf::from("a.txt"),
                mtime_secs: 0,
                size: 0,
            },
            FileFingerprint {
                path: PathBuf::from("sub/b.txt"),
                mtime_secs: 0,
                size: 0,
            },
        ];
        let result = Index::validate_file_paths(&fps);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_file_paths_rejects_absolute_paths() {
        let abs = std::env::current_dir().unwrap().join("a.txt");
        let fps = vec![FileFingerprint {
            path: abs,
            mtime_secs: 0,
            size: 0,
        }];
        let result = Index::validate_file_paths(&fps);
        assert!(result.is_err());
    }

    #[test]
    fn validate_file_paths_rejects_empty_paths() {
        let fps = vec![FileFingerprint {
            path: PathBuf::from(""),
            mtime_secs: 0,
            size: 0,
        }];
        let result = Index::validate_file_paths(&fps);
        assert!(result.is_err());
    }

    #[test]
    fn validate_file_paths_rejects_parent_dir_paths() {
        let fps = vec![FileFingerprint {
            path: PathBuf::from("../escape.txt"),
            mtime_secs: 0,
            size: 0,
        }];
        let result = Index::validate_file_paths(&fps);
        assert!(result.is_err());
    }
}
