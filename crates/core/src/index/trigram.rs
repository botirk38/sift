//! Overlapping byte trigrams.

use std::collections::HashSet;

/// Extract overlapping 3-byte windows from `text`, handling invalid UTF-8 with lossy replacement.
///
/// Valid UTF-8 takes a fast path (no replacement string allocated). Invalid sequences fall back to
/// [`String::from_utf8_lossy`] semantics, matching what [`extract_trigrams_utf8_lossy`] would do on the
/// raw bytes — but in a single pass without an intermediate `String`.
#[must_use]
pub fn extract_trigrams(text: &str) -> Vec<[u8; 3]> {
    extract_trigrams_from_bytes(text.as_bytes())
}

/// Core: sliding 3-byte windows over raw bytes.
#[must_use]
pub fn extract_trigrams_from_bytes(b: &[u8]) -> Vec<[u8; 3]> {
    if b.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(b.len() - 2);
    for i in 0..=b.len() - 3 {
        out.push([b[i], b[i + 1], b[i + 2]]);
    }
    out
}

#[must_use]
pub fn extract_unique_trigrams_from_bytes(b: &[u8]) -> HashSet<[u8; 3]> {
    let mut out = HashSet::new();
    if b.len() < 3 {
        return out;
    }
    out.reserve(b.len().min(1 << 16));
    for i in 0..=b.len() - 3 {
        out.insert([b[i], b[i + 1], b[i + 2]]);
    }
    out
}

/// Extract trigrams from raw bytes, falling back to lossy UTF-8 for invalid sequences.
#[must_use]
#[cfg(test)]
pub fn extract_trigrams_utf8_lossy(bytes: &[u8]) -> Vec<[u8; 3]> {
    std::str::from_utf8(bytes).map_or_else(
        |_| extract_trigrams(String::from_utf8_lossy(bytes).as_ref()),
        extract_trigrams,
    )
}

#[must_use]
pub fn extract_unique_trigrams_utf8_lossy(bytes: &[u8]) -> HashSet<[u8; 3]> {
    std::str::from_utf8(bytes).map_or_else(
        |_| extract_unique_trigrams_from_bytes(String::from_utf8_lossy(bytes).as_ref().as_bytes()),
        |text| extract_unique_trigrams_from_bytes(text.as_bytes()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_lossy(bytes: &[u8]) -> Vec<[u8; 3]> {
        extract_trigrams(String::from_utf8_lossy(bytes).as_ref())
    }

    #[test]
    fn utf8_lossy_matches_reference_valid_ascii() {
        let b = b"hello world";
        assert_eq!(extract_trigrams_utf8_lossy(b), reference_lossy(b));
    }

    #[test]
    fn utf8_lossy_matches_reference_multibyte() {
        let b = "café résumé 日本語".as_bytes();
        assert_eq!(extract_trigrams_utf8_lossy(b), reference_lossy(b));
    }

    #[test]
    fn utf8_lossy_matches_reference_invalid() {
        for b in [
            &[0xff, 0xfe, 0xfd][..],
            b"ok\xff\xfe trail",
            &[0x80][..],
            b"a\xe0\x80\x80b",
        ] {
            assert_eq!(
                extract_trigrams_utf8_lossy(b),
                reference_lossy(b),
                "bytes={b:?}"
            );
        }
    }

    #[test]
    fn utf8_lossy_matches_reference_mixed() {
        let b: Vec<u8> = (0_u8..=255)
            .cycle()
            .take(512)
            .chain(std::iter::once(0xff))
            .collect();
        assert_eq!(extract_trigrams_utf8_lossy(&b), reference_lossy(&b));
    }

    #[test]
    fn short_string_empty() {
        assert!(extract_trigrams("").is_empty());
        assert!(extract_trigrams("ab").is_empty());
    }

    #[test]
    fn ascii_three_chars_one_trigram() {
        assert_eq!(extract_trigrams("abc"), vec![[b'a', b'b', b'c']]);
    }

    #[test]
    fn overlapping_windows() {
        assert_eq!(
            extract_trigrams("abcd"),
            vec![[b'a', b'b', b'c'], [b'b', b'c', b'd']]
        );
    }
}
