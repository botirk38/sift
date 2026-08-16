use std::borrow::Cow;
use std::ops::Range;

use memchr::memchr;

pub(super) struct Line<'a> {
    pub number: Option<u64>,
    pub offset: u64,
    pub bytes: Cow<'a, [u8]>,
}

pub(super) struct Lines<'a> {
    chunk: &'a [u8],
    term: u8,
    numbered: bool,
    number: u64,
    offset: u64,
    pos: usize,
}

impl<'a> Lines<'a> {
    pub(super) const fn new(chunk: &'a [u8], term: u8, numbered: bool) -> Self {
        Self {
            chunk,
            term,
            numbered,
            number: 1,
            offset: 0,
            pos: 0,
        }
    }

    pub(super) fn covering(bytes: &[u8], term: u8, span: Range<usize>) -> Range<usize> {
        let start = memchr::memrchr(term, &bytes[..span.start]).map_or(0, |i| i + 1);
        let end =
            memchr::memchr(term, &bytes[span.end..]).map_or(bytes.len(), |i| span.end + i + 1);
        start..end
    }

    pub(super) fn merge(ranges: &mut Vec<Range<usize>>) {
        if ranges.len() < 2 {
            return;
        }
        ranges.sort_by_key(|range| range.start);
        let mut merged = Vec::with_capacity(ranges.len());
        let mut current = ranges[0].clone();
        for range in ranges.drain(1..) {
            if range.start <= current.end {
                current.end = current.end.max(range.end);
            } else {
                merged.push(current);
                current = range;
            }
        }
        merged.push(current);
        *ranges = merged;
    }

    const fn next_number(&mut self) -> Option<u64> {
        if !self.numbered {
            return None;
        }
        let number = self.number;
        self.number += 1;
        Some(number)
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = Line<'a>;

    fn next(&mut self) -> Option<Line<'a>> {
        if self.pos >= self.chunk.len() {
            return None;
        }
        let start = self.pos;
        let rel =
            memchr(self.term, &self.chunk[start..]).map_or(self.chunk.len() - start, |i| i + 1);
        self.pos = start + rel;
        let offset = self.offset;
        self.offset += rel as u64;
        Some(Line {
            number: self.next_number(),
            offset,
            bytes: Cow::Borrowed(&self.chunk[start..self.pos]),
        })
    }
}

impl Line<'_> {
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn into_owned(self) -> Line<'static> {
        Line {
            number: self.number,
            offset: self.offset,
            bytes: Cow::Owned(self.bytes.into_owned()),
        }
    }

    pub(super) fn without_terminator(&self, term: u8, crlf: bool) -> &[u8] {
        let bytes = self.bytes();
        let Some((last, rest)) = bytes.split_last() else {
            return bytes;
        };
        if *last != term {
            return bytes;
        }
        if crlf
            && let Some((before, stripped)) = rest.split_last()
            && *before == b'\r'
        {
            return stripped;
        }
        rest
    }
}

#[cfg(test)]
mod tests {
    use super::Lines;

    #[test]
    fn splits_on_newline() {
        let lines: Vec<_> = Lines::new(b"a\nb\n", b'\n', true).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].number, Some(1));
        assert_eq!(lines[0].bytes(), b"a\n");
        assert_eq!(lines[1].bytes(), b"b\n");
    }

    #[test]
    fn last_item_may_lack_terminator() {
        let lines: Vec<_> = Lines::new(b"hello\nworld", b'\n', true).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].bytes(), b"world");
        assert_eq!(lines[1].number, Some(2));
    }

    #[test]
    fn without_terminator_strips_crlf() {
        let line = super::Line {
            number: Some(1),
            offset: 0,
            bytes: std::borrow::Cow::Borrowed(b"hi\r\n"),
        };
        assert_eq!(line.without_terminator(b'\n', true), b"hi");
        assert_eq!(line.without_terminator(b'\n', false), b"hi\r");
    }

    #[test]
    fn covering_expands_to_line() {
        let bytes = b"aa\nneedle\nzz\n";
        let span = Lines::covering(bytes, b'\n', 4..8);
        assert_eq!(&bytes[span.clone()], b"needle\n");
        assert_eq!(span, 3..10);
    }
}
