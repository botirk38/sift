use grep_matcher::LineTerminator;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};

use super::{CaseMode, CompiledSearch};

impl CompiledSearch {
    /// # Errors
    /// Returns an error if pattern compilation fails.
    pub fn build_matcher(&self) -> crate::Result<RegexMatcher> {
        let mut builder = RegexMatcherBuilder::new();
        builder.multi_line(true);
        match self.opts.case_mode {
            CaseMode::Sensitive => {}
            CaseMode::Insensitive => {
                builder.case_insensitive(true);
            }
            CaseMode::Smart => {
                builder.case_smart(true);
            }
        }
        builder.fixed_strings(self.opts.fixed_strings());
        if self.opts.word_regexp() {
            builder.word(true);
        }
        if self.opts.line_regexp() {
            builder.whole_line(true);
        }
        builder.line_terminator(Some(b'\n'));
        builder.ban_byte(Some(b'\x00'));
        builder
            .build_many(&self.patterns)
            .map_err(|e| crate::Error::RegexBuild(e.to_string()))
    }

    pub(super) fn build_searcher(&self, line_number: bool, max_matches: Option<usize>) -> Searcher {
        let mut builder = SearcherBuilder::new();
        builder
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_terminator(LineTerminator::byte(b'\n'))
            .invert_match(self.opts.invert_match())
            .line_number(line_number)
            .max_matches(max_matches.map(|n| n as u64));
        builder.build()
    }
}
