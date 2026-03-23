//! Overlapping byte trigrams (UTF-8 bytes of `&str`).

/// Extract overlapping 3-byte windows from `text`.
///
/// Returns an empty vector when `text` has fewer than three UTF-8 bytes.
#[must_use]
pub fn extract_trigrams(text: &str) -> Vec<[u8; 3]> {
    extract_trigrams_bytes(text.as_bytes())
}

/// Trigrams over the same logical text as [`String::from_utf8_lossy`].
///
/// Valid UTF-8 is handled without allocating a replacement string (fast path). Invalid sequences use
/// the lossy replacement rules from the standard library, matching the previous indexer behavior.
#[must_use]
pub fn extract_trigrams_utf8_lossy(bytes: &[u8]) -> Vec<[u8; 3]> {
    std::str::from_utf8(bytes).map_or_else(
        |_| extract_trigrams(String::from_utf8_lossy(bytes).as_ref()),
        extract_trigrams,
    )
}

/// Byte-oriented trigrams (same sliding window as UTF-8 `&str` indexing).
#[must_use]
pub fn extract_trigrams_bytes(b: &[u8]) -> Vec<[u8; 3]> {
    if b.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(b.len() - 2);
    for i in 0..=b.len() - 3 {
        out.push([b[i], b[i + 1], b[i + 2]]);
    }
    out
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
