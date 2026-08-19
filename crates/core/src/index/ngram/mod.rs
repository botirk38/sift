mod build;
pub mod gram;
pub mod storage;

mod candidates;
mod index;
mod literals;

pub use gram::{Gram, GramNorm, GramWidth, GramWindows};
pub use index::{Index, NGramIndexError};

pub(crate) const LEXICON_BIN: &str = "lexicon.bin";

#[cfg(test)]
mod candidate_tests {

    use crate::search::{CaseMode, InputEncoding, Query, SearchFlags, SearchOptions};

    use super::*;

    fn trigram() -> GramWidth {
        GramWidth::TRIGRAM
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

    fn built_query(patterns: &[String], options: SearchOptions) -> Query {
        Query::new(patterns.to_vec(), options).expect("query")
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
        Index::extract_literal_arms(trigram(), &query).is_some()
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
        Index::extract_literal_arms(trigram(), &query).is_none()
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
        let arms = Index::extract_literal_arms(trigram(), &built)
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
        let arms =
            Index::extract_literal_arms(trigram(), &built).expect("fixed casei should extract");
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
        assert!(Index::extract_literal_arms(trigram(), &built).is_none());
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
        assert!(Index::extract_literal_arms(GramWidth::new(2), &built).is_some());
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
        assert!(Index::extract_literal_arms(trigram(), &built).is_some());
    }
}
