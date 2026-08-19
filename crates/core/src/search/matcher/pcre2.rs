use std::ops::Range;

use pcre2::bytes::{Captures, Regex, RegexBuilder};

use crate::SearchError;
use crate::search::event::Replacement;
use crate::search::options::CaseMode;
use crate::search::query::Query;

#[derive(Debug, Clone)]
pub(super) struct Pcre2 {
    regex: Regex,
}

enum Cap<'a> {
    Number(usize),
    Name(&'a str),
}

impl Pcre2 {
    pub(super) fn compile(query: &Query) -> Result<Self, SearchError> {
        let opts = &query.options;
        let mut joined = String::new();
        for (i, pattern) in query.patterns.iter().enumerate() {
            if i > 0 {
                joined.push('|');
            }
            joined.push_str("(?:");
            if opts.fixed_strings() {
                joined.push_str(&pcre2::escape(pattern));
            } else {
                joined.push_str(pattern);
            }
            joined.push(')');
        }
        let caseless = match opts.case_mode {
            CaseMode::Sensitive => false,
            CaseMode::Insensitive => true,
            CaseMode::Smart => Self::smart_caseless(&joined),
        };
        let pattern = if opts.line_regexp() {
            format!("^(?:{joined})$")
        } else if opts.word_regexp() {
            format!(r"(?<!\w)(?:{joined})(?!\w)")
        } else {
            joined
        };

        let mut builder = RegexBuilder::new();
        builder.multi_line(true);
        builder.caseless(caseless);
        builder.utf(opts.unicode);
        builder.ucp(opts.unicode);
        if opts.crlf() {
            builder.crlf(true);
        }
        if opts.multiline() && opts.multiline_dotall() {
            builder.dotall(true);
        }
        let regex = builder
            .build(&pattern)
            .map_err(|err| SearchError::RegexBuild(err.to_string()))?;
        Ok(Self { regex })
    }

    pub(super) fn matched(&self, haystack: &[u8]) -> Result<bool, SearchError> {
        self.regex
            .is_match(haystack)
            .map_err(|err| SearchError::Match(err.to_string()))
    }

    pub(super) fn ranges(&self, haystack: &[u8]) -> Result<Vec<Range<usize>>, SearchError> {
        self.regex
            .find_iter(haystack)
            .map(|m| {
                m.map(|m| m.start()..m.end())
                    .map_err(|err| SearchError::Match(err.to_string()))
            })
            .collect()
    }

    pub(super) fn replace(
        &self,
        haystack: &[u8],
        template: &[u8],
    ) -> Result<Replacement, SearchError> {
        let mut text = Vec::new();
        let mut matches = Vec::new();
        let mut last = 0;
        for caps in self.regex.captures_iter(haystack) {
            let caps = caps.map_err(|err| SearchError::Match(err.to_string()))?;
            let Some(m) = caps.get(0) else { continue };
            text.extend_from_slice(&haystack[last..m.start()]);
            let start = text.len();
            Self::expand(&caps, template, &mut text);
            matches.push(text[start..].to_vec());
            last = m.end();
        }
        text.extend_from_slice(&haystack[last..]);
        Ok(Replacement { text, matches })
    }

    fn expand(caps: &Captures<'_>, template: &[u8], dst: &mut Vec<u8>) {
        let mut rest = template;
        while let Some(i) = rest.iter().position(|&b| b == b'$') {
            dst.extend_from_slice(&rest[..i]);
            rest = &rest[i..];
            if rest.get(1) == Some(&b'$') {
                dst.push(b'$');
                rest = &rest[2..];
                continue;
            }
            let Some((end, cap)) = Cap::parse(rest) else {
                dst.push(b'$');
                rest = &rest[1..];
                continue;
            };
            let captured = match cap {
                Cap::Number(i) => caps.get(i),
                Cap::Name(name) => caps.name(name),
            };
            if let Some(m) = captured {
                dst.extend_from_slice(m.as_bytes());
            }
            rest = &rest[end..];
        }
        dst.extend_from_slice(rest);
    }

    fn smart_caseless(pattern: &str) -> bool {
        let mut chars = pattern.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                chars.next();
            } else if c.is_uppercase() {
                return false;
            }
        }
        true
    }
}

impl Cap<'_> {
    fn parse(template: &[u8]) -> Option<(usize, Cap<'_>)> {
        if template.len() <= 1 || template[0] != b'$' {
            return None;
        }
        let mut i = 1;
        let brace = template.get(1) == Some(&b'{');
        if brace {
            i = 2;
        }
        let mut end = i;
        while template.get(end).is_some_and(|&b| Self::letter(b)) {
            end += 1;
        }
        if end == i {
            return None;
        }
        let name = std::str::from_utf8(&template[i..end]).ok()?;
        if brace {
            if template.get(end) != Some(&b'}') {
                return None;
            }
            end += 1;
        }
        let cap = name
            .parse::<u32>()
            .map_or(Cap::Name(name), |n| Cap::Number(n as usize));
        Some((end, cap))
    }

    const fn letter(b: u8) -> bool {
        matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'_')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::options::{RegexEngine, SearchFlags, SearchOptions};

    fn compile(pattern: &str, options: SearchOptions) -> Pcre2 {
        Pcre2::compile(&Query::new(vec![pattern.into()], options).expect("query")).expect("compile")
    }

    fn pcre2_options() -> SearchOptions {
        SearchOptions {
            regex_engine: RegexEngine::Pcre2,
            ..SearchOptions::default()
        }
    }

    #[test]
    fn word_uses_lookaround_not_ascii_boundaries() {
        let engine = compile(
            "-2",
            SearchOptions {
                flags: SearchFlags::WORD_REGEXP,
                ..pcre2_options()
            },
        );
        assert!(engine.matched(b"abc -2 foo").expect("match"));
    }

    #[test]
    fn smart_case_lowercase_is_insensitive() {
        let engine = compile(
            "abc",
            SearchOptions {
                case_mode: CaseMode::Smart,
                ..pcre2_options()
            },
        );
        assert!(engine.matched(b"ABC").expect("match"));
    }

    #[test]
    fn smart_case_uppercase_literal_is_sensitive() {
        let engine = compile(
            "aBc",
            SearchOptions {
                case_mode: CaseMode::Smart,
                ..pcre2_options()
            },
        );
        assert!(!engine.matched(b"ABC").expect("match"));
    }

    #[test]
    fn crlf_lets_end_anchor_match_before_crlf() {
        let sensitive = compile(
            "abc$",
            SearchOptions {
                flags: SearchFlags::CRLF,
                ..pcre2_options()
            },
        );
        assert!(sensitive.matched(b"abc\r\n").expect("match"));
        let newline = compile("abc$", pcre2_options());
        assert!(!newline.matched(b"abc\r\n").expect("match"));
    }

    #[test]
    fn replace_interpolates_capture_groups() {
        let engine = compile("(foo)(\\d+)", pcre2_options());
        let replacement = engine.replace(b"foo123bar", b"${1}_${2}").expect("replace");
        assert_eq!(replacement.text, b"foo_123bar");
        assert_eq!(replacement.matches, vec![b"foo_123".to_vec()]);
    }
}
