use grep_pcre2::{RegexMatcher as Pcre2Matcher, RegexMatcherBuilder as Pcre2MatcherBuilder};

use crate::SearchError;
use crate::search::options::CaseMode;
use crate::search::query::Query;

pub(super) fn build(query: &Query) -> Result<Pcre2Matcher, SearchError> {
    let opts = &query.options;
    let mut builder = Pcre2MatcherBuilder::new();
    builder.multi_line(true);
    match opts.case_mode {
        CaseMode::Sensitive => {}
        CaseMode::Insensitive => {
            builder.caseless(true);
        }
        CaseMode::Smart => {
            builder.case_smart(true);
        }
    }
    builder.utf(opts.unicode);
    builder.ucp(opts.unicode);
    builder.fixed_strings(opts.fixed_strings());
    if opts.word_regexp() {
        builder.word(true);
    }
    if opts.line_regexp() {
        builder.whole_line(true);
    }
    if opts.crlf() {
        builder.crlf(true);
    }
    if opts.multiline() && opts.multiline_dotall() {
        builder.dotall(true);
    }
    builder
        .build_many(&query.patterns)
        .map_err(|e| SearchError::RegexBuild(e.to_string()))
}
