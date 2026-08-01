use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::gram::{Gram, GramMatch, GramWindows, LiteralNarrowing};
use super::index::Index;
use super::storage::postings::Postings;

/// Dense membership over indexed file ids (one bit per file).
struct FileIdSet {
    words: Vec<u64>,
}

impl FileIdSet {
    fn union_postings(slices: &[&[u8]], file_count: usize) -> Self {
        let mut words = vec![0u64; file_count.div_ceil(64)];
        for slice in slices {
            let ids = Postings::decode_sorted(slice).expect("postings validated at open");
            for id in ids {
                let idx = id as usize;
                if idx < file_count {
                    words[idx / 64] |= 1u64 << (idx % 64);
                }
            }
        }
        Self { words }
    }

    fn contains(&self, id: u32) -> bool {
        let idx = id as usize;
        self.words
            .get(idx / 64)
            .is_some_and(|word| word & (1u64 << (idx % 64)) != 0)
    }

    fn file_ids(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for (word_idx, word) in self.words.iter().enumerate() {
            let mut w = *word;
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                let id = word_idx * 64 + bit;
                if let Ok(id) = u32::try_from(id) {
                    out.push(id);
                }
                w &= w - 1;
            }
        }
        out
    }
}

impl Index {
    pub(crate) fn candidate_file_ids(&self, arms: &[Vec<u8>], gram_match: GramMatch) -> Vec<u32> {
        if arms.is_empty() {
            return Vec::new();
        }
        if arms.len() == 1 {
            return self.posting_ids(&arms[0], gram_match).unwrap_or_default();
        }
        let mut id_lists: Vec<Vec<u32>> = Vec::with_capacity(arms.len());
        for arm in arms {
            if let Some(ids) = self.posting_ids(arm, gram_match) {
                id_lists.push(ids);
            }
        }
        Self::merge_sorted_runs(id_lists)
    }

    fn posting_ids(&self, lit: &[u8], gram_match: GramMatch) -> Option<Vec<u32>> {
        let storage = &self.storage;
        let width = self.width.get();
        match self.width.literal_narrowing(lit.len()) {
            LiteralNarrowing::TooShort => None,
            LiteralNarrowing::Covering => {
                // Union postings for every width-gram that contains `lit`.
                let file_count = storage.files.len();
                let mut window = vec![0u8; width];
                let mut slices: Vec<&[u8]> = Vec::new();
                for wild_pos in 0..width {
                    for b in 0..=u8::MAX {
                        let mut lit_i = 0usize;
                        for (pos, slot) in window.iter_mut().enumerate() {
                            if pos == wild_pos {
                                *slot = b;
                            } else {
                                *slot = lit[lit_i];
                                lit_i += 1;
                            }
                        }
                        for gram in gram_match.grams(&mut window) {
                            let slice = self.posting_bytes_slice(gram);
                            if !slice.is_empty() {
                                slices.push(slice);
                            }
                        }
                    }
                }
                if slices.is_empty() {
                    return None;
                }
                let ids = FileIdSet::union_postings(&slices, file_count).file_ids();
                if ids.is_empty() { None } else { Some(ids) }
            }
            LiteralNarrowing::Windows => match gram_match {
                GramMatch::Exact => {
                    let grams: Vec<Gram> = GramWindows::new(lit, self.width).collect();
                    if grams.is_empty() {
                        return None;
                    }
                    let mut slices: Vec<&[u8]> = Vec::with_capacity(grams.len());
                    for gram in &grams {
                        let s = self.posting_bytes_slice(*gram);
                        if s.is_empty() {
                            return None;
                        }
                        slices.push(s);
                    }
                    let ids = Self::intersect_sorted_slices(&slices);
                    if ids.is_empty() { None } else { Some(ids) }
                }
                GramMatch::AsciiCase => {
                    let file_count = storage.files.len();
                    let mut window = vec![0u8; width];
                    let mut cur: Option<Vec<u32>> = None;
                    for offset in 0..=lit.len() - width {
                        window.copy_from_slice(&lit[offset..offset + width]);
                        let mut slices: Vec<&[u8]> = Vec::new();
                        for gram in gram_match.grams(&mut window) {
                            let slice = self.posting_bytes_slice(gram);
                            if !slice.is_empty() {
                                slices.push(slice);
                            }
                        }
                        if slices.is_empty() {
                            return None;
                        }
                        // Single posting list: decode directly on the first window.
                        cur = Some(if cur.is_none() && slices.len() == 1 {
                            Postings::decode_sorted(slices[0]).expect("postings validated at open")
                        } else {
                            let set = FileIdSet::union_postings(&slices, file_count);
                            cur.map_or_else(
                                || set.file_ids(),
                                |prev| prev.into_iter().filter(|&id| set.contains(id)).collect(),
                            )
                        });
                        if cur.as_ref().is_some_and(Vec::is_empty) {
                            return None;
                        }
                    }
                    cur.filter(|ids| !ids.is_empty())
                }
            },
        }
    }

    fn posting_bytes_slice(&self, gram: Gram) -> &[u8] {
        let storage = &self.storage;
        let Some(entry) = storage.lexicon.get(gram) else {
            return &[];
        };
        let start = usize::try_from(entry.offset).unwrap_or(usize::MAX);
        let payload_len = storage.postings.payload_len();
        let end = storage.lexicon.posting_byte_end(entry.offset, payload_len);
        storage.postings.slice(start, end.saturating_sub(start))
    }

    pub(crate) fn intersect_sorted_slices(slices: &[&[u8]]) -> Vec<u32> {
        if slices.is_empty() {
            return Vec::new();
        }
        if slices.len() == 1 {
            return Postings::decode_sorted(slices[0]).expect("postings validated at open");
        }
        if slices.len() == 2 {
            let (first, second) = if slices[0].len() <= slices[1].len() {
                (slices[0], slices[1])
            } else {
                (slices[1], slices[0])
            };
            if first == second {
                return Postings::decode_sorted(first).expect("postings validated at open");
            }
            let ids = Postings::decode_sorted(first).expect("postings validated at open");
            return Postings::intersect_sorted(&ids, second).expect("postings validated at open");
        }
        let mut ordered: Vec<&[u8]> = slices.to_vec();
        ordered.sort_unstable_by_key(|slice| slice.len());
        if ordered[1..].iter().all(|slice| *slice == ordered[0]) {
            return Postings::decode_sorted(ordered[0]).expect("postings validated at open");
        }
        let mut cur = Postings::decode_sorted(ordered[0]).expect("postings validated at open");
        for s in &ordered[1..] {
            cur = Postings::intersect_sorted(&cur, s).expect("postings validated at open");
            if cur.is_empty() {
                break;
            }
        }
        cur
    }

    pub(crate) fn merge_sorted_runs(lists: Vec<Vec<u32>>) -> Vec<u32> {
        if lists.is_empty() {
            return Vec::new();
        }
        if lists.len() == 1 {
            return lists.into_iter().next().unwrap_or_default();
        }

        let total: usize = lists.iter().map(Vec::len).sum();
        let mut heap: BinaryHeap<Reverse<(u32, usize)>> = BinaryHeap::with_capacity(lists.len());
        let mut positions = vec![0usize; lists.len()];

        for (list_idx, list) in lists.iter().enumerate() {
            if let Some(&first) = list.first() {
                heap.push(Reverse((first, list_idx)));
            }
        }

        let mut out = Vec::with_capacity(total);
        let mut last = None;
        while let Some(Reverse((value, list_idx))) = heap.pop() {
            if last != Some(value) {
                out.push(value);
                last = Some(value);
            }

            positions[list_idx] += 1;
            if let Some(&next) = lists[list_idx].get(positions[list_idx]) {
                heap.push(Reverse((next, list_idx)));
            }
        }
        out
    }
}
