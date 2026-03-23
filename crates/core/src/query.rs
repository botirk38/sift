//! Resolve posting lists into candidate file paths.
//!
//! Sorted posting intersections are merge-joins (not in `std`); kept as private helpers on [`Index`].

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::planner::Arm;
use crate::Index;

fn u32_vec_from_le_bytes(slice: &[u8]) -> Vec<u32> {
    if !slice.len().is_multiple_of(4) {
        return Vec::new();
    }
    slice
        .chunks_exact(4)
        .filter_map(|c| <[u8; 4]>::try_from(c).ok().map(u32::from_le_bytes))
        .collect()
}

impl Index {
    /// Raw little-endian `u32` posting payload for `tri` (sorted ids), or empty if absent.
    #[must_use]
    fn posting_bytes_slice(&self, tri: [u8; 3]) -> &[u8] {
        let Ok(idx) = self.lexicon.binary_search_by_key(&tri, |e| e.trigram) else {
            return &[];
        };
        let e = &self.lexicon[idx];
        let Ok(start) = usize::try_from(e.offset) else {
            return &[];
        };
        let Ok(n) = usize::try_from(e.len) else {
            return &[];
        };
        let Some(nbytes) = n.checked_mul(4) else {
            return &[];
        };
        let Some(end) = start.checked_add(nbytes) else {
            return &[];
        };
        self.postings.get(start..end).unwrap_or(&[])
    }

    /// Look up sorted posting list for `tri`, or empty if absent.
    #[must_use]
    pub fn posting_list_for_trigram(&self, tri: [u8; 3]) -> Vec<u32> {
        u32_vec_from_le_bytes(self.posting_bytes_slice(tri))
    }

    /// Union candidate file ids across OR arms, then map to corpus-relative paths.
    #[must_use]
    pub fn candidate_paths(&self, arms: &[Arm]) -> HashSet<PathBuf> {
        let mut id_lists: Vec<Vec<u32>> = Vec::with_capacity(arms.len());
        for arm in arms {
            if arm.is_empty() {
                continue;
            }
            let slices: Vec<&[u8]> = arm
                .iter()
                .map(|tri| self.posting_bytes_slice(*tri))
                .collect();
            if slices.iter().any(|s| s.is_empty()) {
                continue;
            }
            let ids = Self::intersect_sorted_posting_byte_slices(&slices);
            if !ids.is_empty() {
                id_lists.push(ids);
            }
        }
        let refs: Vec<&[u32]> = id_lists.iter().map(Vec::as_slice).collect();
        let union_ids = Self::union_sorted_runs(&refs);
        let mut out = HashSet::new();
        for id in union_ids {
            if let Some(p) = self.files.get(id as usize) {
                out.insert(p.clone());
            }
        }
        out
    }

    fn intersect_sorted_posting_byte_slices(slices: &[&[u8]]) -> Vec<u32> {
        if slices.is_empty() {
            return Vec::new();
        }
        if slices.len() == 1 {
            return u32_vec_from_le_bytes(slices[0]);
        }
        let mut cur = Self::intersect_two_posting_bytes(slices[0], slices[1]);
        for s in &slices[2..] {
            cur = Self::intersect_vec_with_posting_bytes(&cur, s);
            if cur.is_empty() {
                break;
            }
        }
        cur
    }

    fn intersect_two_posting_bytes(a: &[u8], b: &[u8]) -> Vec<u32> {
        if !a.len().is_multiple_of(4) || !b.len().is_multiple_of(4) {
            return Vec::new();
        }
        let an = a.len() / 4;
        let bn = b.len() / 4;
        let mut i = 0usize;
        let mut j = 0usize;
        let mut out = Vec::new();
        while i < an && j < bn {
            let ai = u32::from_le_bytes(a[i * 4..i * 4 + 4].try_into().unwrap());
            let bj = u32::from_le_bytes(b[j * 4..j * 4 + 4].try_into().unwrap());
            match ai.cmp(&bj) {
                Ordering::Less => i += 1,
                Ordering::Greater => j += 1,
                Ordering::Equal => {
                    out.push(ai);
                    i += 1;
                    j += 1;
                }
            }
        }
        out
    }

    fn intersect_vec_with_posting_bytes(cur: &[u32], b: &[u8]) -> Vec<u32> {
        if !b.len().is_multiple_of(4) {
            return Vec::new();
        }
        let bn = b.len() / 4;
        let mut i = 0usize;
        let mut j = 0usize;
        let mut out = Vec::new();
        while i < cur.len() && j < bn {
            let bj = u32::from_le_bytes(b[j * 4..j * 4 + 4].try_into().unwrap());
            match cur[i].cmp(&bj) {
                Ordering::Less => i += 1,
                Ordering::Greater => j += 1,
                Ordering::Equal => {
                    out.push(cur[i]);
                    i += 1;
                    j += 1;
                }
            }
        }
        out
    }

    #[cfg(test)]
    fn intersect_two_sorted(a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut i = 0usize;
        let mut j = 0usize;
        let mut out = Vec::new();
        while i < a.len() && j < b.len() {
            match a[i].cmp(&b[j]) {
                Ordering::Less => i += 1,
                Ordering::Greater => j += 1,
                Ordering::Equal => {
                    out.push(a[i]);
                    i += 1;
                    j += 1;
                }
            }
        }
        out
    }

    #[cfg(test)]
    fn intersect_sorted_runs(lists: &[&[u32]]) -> Vec<u32> {
        if lists.is_empty() {
            return Vec::new();
        }
        if lists.len() == 1 {
            return lists[0].to_vec();
        }
        let mut cur = lists[0].to_vec();
        for next in &lists[1..] {
            cur = Self::intersect_two_sorted(&cur, next);
            if cur.is_empty() {
                break;
            }
        }
        cur
    }

    fn union_sorted_runs(lists: &[&[u32]]) -> Vec<u32> {
        if lists.is_empty() {
            return Vec::new();
        }
        let total: usize = lists.iter().map(|s| s.len()).sum();
        let mut all: Vec<u32> = Vec::with_capacity(total);
        for s in lists {
            all.extend_from_slice(s);
        }
        all.sort_unstable();
        all.dedup();
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::lexicon::LexiconEntry;

    #[test]
    fn missing_trigram_empty_list() {
        let lex = vec![LexiconEntry {
            trigram: *b"abc",
            offset: 0,
            len: 1,
        }];
        let postings = 4u32.to_le_bytes();
        let index = crate::Index::test_stub(lex, postings.to_vec());
        assert!(index.posting_list_for_trigram(*b"zzz").is_empty());
    }

    #[test]
    fn intersect_two_sorted_merge() {
        let a = [1u32, 3, 5, 7];
        let b = [2u32, 3, 5, 9];
        assert_eq!(Index::intersect_two_sorted(&a, &b), vec![3, 5]);
    }

    #[test]
    fn intersect_many_lists() {
        let a = [1u32, 2, 3];
        let b = [2u32, 3, 4];
        let c = [3u32];
        let lists = [&a[..], &b[..], &c[..]];
        assert_eq!(Index::intersect_sorted_runs(&lists), vec![3]);
    }

    #[test]
    fn union_sorted_dedup() {
        let a = [1u32, 3];
        let b = [3u32, 4];
        assert_eq!(Index::union_sorted_runs(&[&a[..], &b[..]]), vec![1, 3, 4]);
    }
}
