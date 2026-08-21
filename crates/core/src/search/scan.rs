use std::collections::VecDeque;

use crate::search::error::Error as SearchError;
use crate::search::event::ContextKind;
use crate::search::line::{Break, Line, Lines, Multiline};
use crate::search::matcher::{Matcher, Scratch};
use crate::search::mode::{Hit, SearchMode};
use crate::search::options::{Nul, SearchOptions};

pub(super) enum Item {
    Hit(Line<'static>),
    Context {
        line: Line<'static>,
        kind: ContextKind,
    },
    Break,
}

/// One-file search walk. Yields hits/context; does not know listing or events.
pub(super) struct FileScan<'h> {
    matcher: &'h Matcher,
    options: &'h SearchOptions,
    mode: SearchMode,
    chunk: &'h [u8],
    brk: Break,
    nul: Nul,
    limit: Option<usize>,
    multiline: Option<Multiline>,
    lines: Lines<'h>,
    before: VecDeque<Line<'static>>,
    pending: VecDeque<Item>,
    after_left: usize,
    emitted: bool,
    prev: u64,
    accepted: usize,
    matched: bool,
    hits: usize,
    binary: Option<u64>,
    bytes: u64,
    finished: bool,
    before_n: usize,
    after_n: usize,
    scratch: Scratch,
}

impl<'h> FileScan<'h> {
    pub(super) fn new(
        chunk: &'h [u8],
        matcher: &'h Matcher,
        options: &'h SearchOptions,
        mode: SearchMode,
        nul: Nul,
        binary: Option<u64>,
    ) -> Result<Self, SearchError> {
        let before_n = if options.passthru() {
            0
        } else {
            options.before_context
        };
        let after_n = if options.passthru() {
            0
        } else {
            options.after_context
        };
        let limit = mode
            .is_path_mode()
            .then_some(1usize)
            .or(options.max_results);
        let mut scratch = matcher.scratch();
        let mut multiline = if options.multiline() {
            let template = options.replace.as_deref().map(str::as_bytes);
            Some(Multiline::new(matcher.spans(
                chunk,
                template,
                &mut scratch,
            )?))
        } else {
            None
        };
        if matches!(mode.hit(), Some(Hit::Span))
            && let (Some(n), Some(spans)) = (limit, multiline.as_mut())
        {
            spans.keep_first(n);
        }
        let brk = if matches!(nul, Nul::Convert) && !options.multiline() {
            Break::TermOrNul(options.line_terminator())
        } else {
            Break::Term(options.line_terminator())
        };
        Ok(Self {
            matcher,
            options,
            mode,
            chunk,
            brk,
            nul,
            limit,
            multiline,
            lines: Lines::new(chunk, brk, !mode.is_path_mode()),
            before: VecDeque::new(),
            pending: VecDeque::new(),
            after_left: 0,
            emitted: false,
            prev: 0,
            accepted: 0,
            matched: false,
            hits: 0,
            binary,
            bytes: u64::try_from(chunk.len()).unwrap_or(u64::MAX),
            finished: false,
            before_n,
            after_n,
            scratch,
        })
    }

    pub(super) const fn matched(&self) -> bool {
        self.matched
    }

    pub(super) const fn hits(&self) -> usize {
        self.hits
    }

    pub(super) const fn bytes_searched(&self) -> u64 {
        self.bytes
    }

    pub(super) const fn binary(&self) -> Option<u64> {
        self.binary
    }

    pub(super) const fn slice(&self) -> &[u8] {
        self.chunk
    }

    pub(super) const fn spans(&self) -> Option<&Multiline> {
        if self.options.invert_match() {
            None
        } else {
            self.multiline.as_ref()
        }
    }

    pub(super) const fn scratch(&mut self) -> &mut Scratch {
        &mut self.scratch
    }

    const fn span_limited(&self) -> bool {
        matches!(self.mode.hit(), Some(Hit::Span)) && self.multiline.is_some()
    }

    pub(super) fn next_item(&mut self) -> Option<Result<Item, SearchError>> {
        loop {
            if let Some(item) = self.pending.pop_front() {
                return Some(Ok(item));
            }
            if self.finished {
                return None;
            }
            match self.pump() {
                Ok(true) => {}
                Ok(false) => {
                    self.finish_hits();
                    self.finished = true;
                }
                Err(err) => {
                    self.finished = true;
                    return Some(Err(err));
                }
            }
        }
    }

    fn pump(&mut self) -> Result<bool, SearchError> {
        let Some(line) = self.lines.next() else {
            return Ok(false);
        };
        if matches!(self.nul, Nul::Quit)
            && let Some(idx) = memchr::memchr(0, line.bytes())
        {
            self.binary
                .get_or_insert_with(|| line.offset + u64::try_from(idx).unwrap_or(u64::MAX));
            return Ok(false);
        }
        if self.options.replace.is_some()
            && !self.options.invert_match()
            && let Some(spans) = self.multiline.as_ref()
            && spans.consumed(&line, self.options.line_terminator(), self.options.crlf())
        {
            return Ok(true);
        }
        let matched = match self.multiline.as_ref() {
            None => self.matcher.matched(
                line.without_break(self.brk, self.options.crlf()),
                &mut self.scratch,
            )?,
            Some(spans) => spans.overlaps(&line),
        };
        let hit = matched != self.options.invert_match();
        let accepting = self.span_limited() || self.limit.is_none_or(|n| self.accepted < n);
        if hit && accepting {
            if (self.before_n > 0 || self.after_n > 0) && self.emitted && self.prev < line.offset {
                self.pending.push_back(Item::Break);
            }
            while let Some(held) = self.before.pop_front() {
                self.pending.push_back(Item::Context {
                    line: held,
                    kind: ContextKind::Before,
                });
            }
            self.record_hit(&line)?;
            let end = line.end();
            self.pending.push_back(Item::Hit(line.into_owned()));
            self.accepted += 1;
            self.after_left = self.after_n;
            self.emitted = true;
            self.prev = end;
            if !self.span_limited()
                && self.limit.is_some_and(|n| self.accepted >= n)
                && self.after_left == 0
            {
                return Ok(false);
            }
            return Ok(true);
        }
        if self.options.passthru() {
            self.prev = line.end();
            self.emitted = true;
            self.pending.push_back(Item::Context {
                line: line.into_owned(),
                kind: ContextKind::Other,
            });
            return Ok(true);
        }
        if self.after_left > 0 {
            self.prev = line.end();
            self.emitted = true;
            self.after_left -= 1;
            self.pending.push_back(Item::Context {
                line: line.into_owned(),
                kind: ContextKind::After,
            });
            if !self.span_limited()
                && self.limit.is_some_and(|n| self.accepted >= n)
                && self.after_left == 0
            {
                return Ok(false);
            }
            return Ok(true);
        }
        if self.before_n > 0 {
            if self.before.len() == self.before_n {
                self.before.pop_front();
            }
            self.before.push_back(line.into_owned());
        }
        Ok(true)
    }

    fn record_hit(&mut self, line: &Line<'_>) -> Result<(), SearchError> {
        self.matched = true;
        if matches!(self.mode.hit(), Some(Hit::Span))
            && !self.options.invert_match()
            && self.multiline.is_none()
        {
            self.hits = self.hits.saturating_add(
                self.matcher
                    .spans(line.bytes(), None, &mut self.scratch)?
                    .len(),
            );
        } else if !(matches!(self.mode.hit(), Some(Hit::Span))
            && !self.options.invert_match()
            && self.multiline.is_some())
        {
            self.hits = self.hits.saturating_add(1);
        }
        Ok(())
    }

    const fn finish_hits(&mut self) {
        if matches!(self.mode.hit(), Some(Hit::Span))
            && !self.options.invert_match()
            && let Some(spans) = self.multiline.as_ref()
        {
            self.hits = spans.count();
        }
    }
}
