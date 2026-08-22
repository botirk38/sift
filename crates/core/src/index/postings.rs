//! Posting-list payloads shared by index kinds.
//!
//! Each kind stores its own `postings.bin` in its snapshot namespace and
//! addresses lists by `(offset, len)` from whatever directory it keeps.
//!
//! Each posting list is a self-describing blob: a varint element count, then
//! whole 128-value blocks SIMD delta-bitpacked via [`BitPacker4x`], then a
//! delta-varint tail for the remaining `< 128` values.

use std::cmp::Ordering;
use std::path::Path;

use bitpacking::{BitPacker, BitPacker4x};
use integer_encoding::VarInt;

use crate::index::mmap::mmap_open;

const POSTINGS_MAGIC: [u8; 8] = *b"SIFTPST3";

/// On-disk artifact name for a kind's posting payload.
pub const POSTINGS_BIN: &str = "postings.bin";

/// Values per SIMD-bitpacked block.
const BLOCK_LEN: usize = BitPacker4x::BLOCK_LEN;

#[derive(Debug)]
pub struct Postings {
    data: memmap2::Mmap,
    payload_len: usize,
}

impl Postings {
    fn bytes(&self) -> &[u8] {
        &self.data
    }

    fn malformed(msg: &'static str) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::InvalidData, msg)
    }

    fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .expect("slice is exactly 4 bytes"),
        )
    }

    /// Encode a postings payload into bytes (magic + length prefix + payload).
    ///
    /// # Errors
    ///
    /// Returns an error if the payload length exceeds `u32::MAX`.
    pub fn encode(payload: &[u8]) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::with_capacity(POSTINGS_MAGIC.len() + 4 + payload.len());
        data.extend_from_slice(&POSTINGS_MAGIC);
        let plen = u32::try_from(payload.len()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "postings payload exceeds u32::MAX",
            )
        })?;
        data.extend_from_slice(&plen.to_le_bytes());
        data.extend_from_slice(payload);
        Ok(data)
    }

    /// Write a postings file and return an mmap-backed instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written or reopened.
    pub fn create(path: &Path, payload: &[u8]) -> std::io::Result<Self> {
        let data = Self::encode(payload)?;
        std::fs::write(path, &data)?;
        Self::open(path)
    }

    /// Open postings from a memory-mapped file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file is malformed.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let mmap = mmap_open(path)?;
        let payload_len = Self::validate(&mmap)?;
        Ok(Self {
            data: mmap,
            payload_len,
        })
    }

    fn validate(bytes: &[u8]) -> std::io::Result<usize> {
        let magic_len = POSTINGS_MAGIC.len();
        if bytes.len() < magic_len + 4 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "postings too short for magic+len",
            ));
        }
        if bytes[..magic_len] != POSTINGS_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unexpected postings magic",
            ));
        }
        let plen = Self::read_u32_le(bytes, magic_len) as usize;
        if bytes.len() < magic_len + 4 + plen {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "postings payload shorter than declared length",
            ));
        }
        if bytes.len() > magic_len + 4 + plen {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "postings has trailing bytes after declared payload",
            ));
        }
        Ok(plen)
    }

    /// Encode a strictly increasing file-id list into a self-describing blob.
    ///
    /// Full 128-value blocks are delta-bitpacked; the remainder is delta-varint
    /// encoded. Callers guarantee `ids` is sorted and unique (posting lists are
    /// assembled from the sorted `(gram, file id)` pairs).
    pub(crate) fn encode_list(ids: &[u32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(ids.len() + 8);
        let mut buf = [0u8; 10];
        let count = u64::try_from(ids.len()).expect("list length fits u64");
        let n = count.encode_var(&mut buf);
        out.extend_from_slice(&buf[..n]);
        if ids.is_empty() {
            return out;
        }

        let bitpacker = BitPacker4x::new();
        let full = ids.len() / BLOCK_LEN;
        let mut initial = 0u32;
        let mut block_buf = [0u8; BLOCK_LEN * 4];
        for b in 0..full {
            let block = &ids[b * BLOCK_LEN..(b + 1) * BLOCK_LEN];
            let num_bits = bitpacker.num_bits_sorted(initial, block);
            out.push(num_bits);
            let written = bitpacker.compress_sorted(initial, block, &mut block_buf, num_bits);
            out.extend_from_slice(&block_buf[..written]);
            initial = block[BLOCK_LEN - 1];
        }

        let mut prev = u64::from(initial);
        for &value in &ids[full * BLOCK_LEN..] {
            let raw = u64::from(value)
                .checked_sub(prev)
                .expect("posting ids strictly increasing");
            let n = raw.encode_var(&mut buf);
            out.extend_from_slice(&buf[..n]);
            prev = u64::from(value);
        }
        out
    }

    #[must_use]
    pub const fn payload_len(&self) -> usize {
        self.payload_len
    }

    #[must_use]
    pub fn slice(&self, start: usize, len: usize) -> &[u8] {
        let payload_start = POSTINGS_MAGIC.len() + 4;
        let start = payload_start + start;
        self.bytes().get(start..start + len).unwrap_or(&[])
    }

    /// Decode a posting list into its sorted values.
    ///
    /// Rejects truncated blocks, oversized `num_bits`, malformed tail varints,
    /// delta overflow, values exceeding `u32::MAX`, and trailing bytes.
    pub(crate) fn decode_sorted(bytes: &[u8]) -> std::io::Result<Vec<u32>> {
        let (count, mut pos) =
            u64::decode_var(bytes).ok_or_else(|| Self::malformed("malformed varint"))?;
        let count =
            usize::try_from(count).map_err(|_| Self::malformed("posting count exceeds usize"))?;
        if count == 0 {
            if pos != bytes.len() {
                return Err(Self::malformed("trailing bytes after empty posting list"));
            }
            return Ok(Vec::new());
        }

        // Reject implausible counts before reserving so a corrupt blob cannot
        // trigger a huge allocation: each full block needs at least its 1-byte
        // header and each tail value at least a 1-byte varint, so `count` values
        // require at least `full + tail` bytes of payload.
        let full = count / BLOCK_LEN;
        let tail = count % BLOCK_LEN;
        if full + tail > bytes.len() - pos {
            return Err(Self::malformed("posting count exceeds payload size"));
        }

        let mut out = Vec::with_capacity(count);
        let bitpacker = BitPacker4x::new();
        let mut initial = 0u32;
        let mut block_buf = [0u32; BLOCK_LEN];
        for _ in 0..full {
            let num_bits = *bytes
                .get(pos)
                .ok_or_else(|| Self::malformed("truncated posting block header"))?;
            pos += 1;
            if num_bits > 32 {
                return Err(Self::malformed("posting block num_bits exceeds 32"));
            }
            let block_bytes = num_bits as usize * BLOCK_LEN / 8;
            let compressed = bytes
                .get(pos..pos + block_bytes)
                .ok_or_else(|| Self::malformed("truncated posting block"))?;
            pos += block_bytes;
            bitpacker.decompress_sorted(initial, compressed, &mut block_buf, num_bits);
            out.extend_from_slice(&block_buf);
            initial = block_buf[BLOCK_LEN - 1];
        }

        let mut prev = u64::from(initial);
        for _ in 0..tail {
            let (raw, consumed) = u64::decode_var(&bytes[pos..])
                .ok_or_else(|| Self::malformed("malformed varint"))?;
            pos += consumed;
            let value = prev
                .checked_add(raw)
                .ok_or_else(|| Self::malformed("delta overflow in posting list"))?;
            if value > u64::from(u32::MAX) {
                return Err(Self::malformed("posting value exceeds u32::MAX"));
            }
            out.push(u32::try_from(value).expect("value bounded above"));
            prev = value;
        }

        if pos != bytes.len() {
            return Err(Self::malformed("trailing bytes after posting list"));
        }
        Ok(out)
    }

    pub(crate) fn intersect_sorted(ids: &[u32], encoded: &[u8]) -> std::io::Result<Vec<u32>> {
        let other = Self::decode_sorted(encoded)?;
        Ok(Self::intersect_slices(ids, &other))
    }

    /// Intersect two ascending, unique id slices via a linear merge.
    pub(crate) fn intersect_slices(a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(a.len().min(b.len()));
        let (mut i, mut j) = (0usize, 0usize);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn create_and_open_roundtrips() {
        let tmp = TempDir::new().expect("create temp dir");
        let path = tmp.path().join("postings.bin");
        let payload = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let postings = Postings::create(&path, &payload).expect("create");
        assert_eq!(postings.payload_len(), payload.len());
        let slice = postings.slice(0, payload.len());
        assert_eq!(slice, payload.as_slice());
    }

    #[test]
    fn slice_returns_requested_range() {
        let tmp = TempDir::new().expect("create temp dir");
        let path = tmp.path().join("postings.bin");
        let payload: Vec<u8> = (0..16).collect();
        let postings = Postings::create(&path, &payload).expect("create");
        let slice = postings.slice(4, 8);
        assert_eq!(slice, &payload[4..12]);
    }

    #[test]
    fn open_rejects_bad_magic() {
        let tmp = TempDir::new().expect("create temp dir");
        let path = tmp.path().join("postings.bin");
        std::fs::write(&path, b"BADMAGIC").expect("write");
        let result = Postings::open(&path);
        assert!(result.is_err());
    }

    #[test]
    fn open_rejects_declared_payload_longer_than_file() {
        let tmp = TempDir::new().expect("create temp dir");
        let path = tmp.path().join("postings.bin");
        let mut data = POSTINGS_MAGIC.to_vec();
        data.extend_from_slice(&100u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]);
        std::fs::write(&path, &data).expect("write");
        let result = Postings::open(&path);
        assert!(result.is_err());
    }

    #[test]
    fn encode_decode_roundtrips() {
        let ids = vec![0u32, 1, 5, 100, 10_000];
        let encoded = Postings::encode_list(&ids);
        let decoded = Postings::decode_sorted(&encoded).expect("decode");
        assert_eq!(decoded, ids);
    }

    #[test]
    fn encode_decode_roundtrips_across_blocks() {
        // Exercises full bitpacked blocks plus a varint tail (300 = 2*128 + 44).
        let ids: Vec<u32> = (0..300u32).map(|i| i * 3 + 7).collect();
        let encoded = Postings::encode_list(&ids);
        let decoded = Postings::decode_sorted(&encoded).expect("decode");
        assert_eq!(decoded, ids);
    }

    #[test]
    fn encode_decode_roundtrips_exact_block_multiple() {
        let ids: Vec<u32> = (0..256u32).map(|i| i * 5).collect();
        let encoded = Postings::encode_list(&ids);
        assert_eq!(Postings::decode_sorted(&encoded).expect("decode"), ids);
    }

    #[test]
    fn encode_decode_empty() {
        let encoded = Postings::encode_list(&[]);
        assert!(
            Postings::decode_sorted(&encoded)
                .expect("decode")
                .is_empty()
        );
    }

    #[test]
    fn intersect_works() {
        let left = vec![1u32, 3, 5, 7];
        let encoded = Postings::encode_list(&[2u32, 3, 6, 7]);
        let result = Postings::intersect_sorted(&left, &encoded).expect("intersect");
        assert_eq!(result, vec![3, 7]);
    }

    #[test]
    fn intersect_works_across_blocks() {
        let left: Vec<u32> = (0..400u32).filter(|i| i % 2 == 0).collect();
        let encoded =
            Postings::encode_list(&(0..400u32).filter(|i| i % 3 == 0).collect::<Vec<_>>());
        let result = Postings::intersect_sorted(&left, &encoded).expect("intersect");
        let expected: Vec<u32> = (0..400u32).filter(|i| i % 6 == 0).collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn decode_rejects_malformed_varint() {
        let result = Postings::decode_sorted(&[0xff]);
        assert!(result.is_err());
    }

    #[test]
    fn open_rejects_trailing_bytes() {
        let tmp = TempDir::new().expect("create temp dir");
        let path = tmp.path().join("postings.bin");
        let mut data = POSTINGS_MAGIC.to_vec();
        data.extend_from_slice(&4u32.to_le_bytes()); // declares payload length 4
        data.extend_from_slice(b"abcd");
        data.extend_from_slice(b"TRAILING"); // extra bytes
        std::fs::write(&path, &data).expect("write");
        let result = Postings::open(&path);
        assert!(result.is_err());
    }

    #[test]
    fn decode_rejects_truncated_varint() {
        let result = Postings::decode_sorted(&[0x80, 0x80, 0x80]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_rejects_count_larger_than_payload() {
        // A corrupt blob declaring a huge count with no payload must be rejected
        // cheaply rather than reserving for that many values.
        let buf = u64::MAX.encode_var_vec();
        let result = Postings::decode_sorted(&buf);
        assert!(result.is_err());
    }

    #[test]
    fn decode_rejects_value_exceeding_u32_max() {
        // count = 1 (all tail), then a tail delta producing value > u32::MAX.
        let mut buf = Vec::new();
        buf.extend_from_slice(&1u64.encode_var_vec());
        buf.extend_from_slice(&(u64::from(u32::MAX) + 1).encode_var_vec());
        let result = Postings::decode_sorted(&buf);
        assert!(result.is_err());
    }
}
