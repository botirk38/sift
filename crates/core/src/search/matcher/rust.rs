use grep_regex::{RegexMatcher, RegexMatcherBuilder};

use crate::SearchError;
use crate::search::options::CaseMode;
use crate::search::query::Query;

pub(super) fn build(query: &Query) -> Result<RegexMatcher, SearchError> {
    let opts = &query.options;
    let mut builder = RegexMatcherBuilder::new();
    builder.multi_line(true);
    match opts.case_mode {
        CaseMode::Sensitive => {}
        CaseMode::Insensitive => {
            builder.case_insensitive(true);
        }
        CaseMode::Smart => {
            builder.case_smart(true);
        }
    }
    builder.unicode(opts.unicode);
    builder.fixed_strings(opts.fixed_strings());
    if opts.word_regexp() {
        builder.word(true);
    }
    if opts.line_regexp() {
        builder.whole_line(true);
    }
    if opts.regex_size_limit > 0 {
        builder.size_limit(opts.regex_size_limit);
    }
    if opts.dfa_size_limit > 0 {
        builder.dfa_size_limit(opts.dfa_size_limit);
    }
    if opts.crlf() {
        builder.crlf(true);
    }
    if opts.multiline() {
        if opts.multiline_dotall() {
            builder.dot_matches_new_line(true);
        }
    } else {
        builder.line_terminator(Some(opts.line_terminator()));
    }
    builder.ban_byte(None);
    builder
        .build_many(&query.patterns)
        .map_err(|e| SearchError::RegexBuild(e.to_string()))
}
