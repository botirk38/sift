use std::borrow::Cow;
use std::path::Path;

use encoding_rs::Encoding;
use fastio::OwnedBytes;
use memchr::memchr;

use crate::search::input::Input;
use crate::search::options::{InputEncoding, Io};

enum Source<'a> {
    Slice(&'a [u8]),
    Memory(OwnedBytes),
}

pub(super) struct Bytes<'a> {
    source: Source<'a>,
}

impl<'a> Bytes<'a> {
    pub(super) fn open(input: &'a Input<'a>, io: Io) -> std::io::Result<Self> {
        match input {
            Input::Bytes { bytes, .. } => Ok(Self {
                source: Source::Slice(bytes),
            }),
            Input::Path { origin, .. } => {
                let path = origin.abs_path().ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "path input has no file")
                })?;
                Ok(Self {
                    source: Source::Memory(Self::read_path(path, io)?),
                })
            }
        }
    }

    fn read_path(path: &Path, io: Io) -> std::io::Result<OwnedBytes> {
        match io {
            Io::Sync => fastio::sync::File::open(path)?.read_all(),
            Io::Mmap => {
                let file = fastio::mmap::File::open(path)?;
                if file.metadata()?.len() == 0 {
                    Ok(OwnedBytes::from_vec(Vec::new()))
                } else {
                    Ok(OwnedBytes::from(file.map()?))
                }
            }
            Io::Uring => Self::read_all_uring(path),
        }
    }

    fn read_all_uring(path: &Path) -> std::io::Result<OwnedBytes> {
        #[cfg(target_os = "linux")]
        {
            fastio::uring::File::open(path)?.read_all()
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = path;
            Err(std::io::Error::other("io uring is only available on Linux"))
        }
    }

    pub(super) fn as_slice(&self) -> &[u8] {
        match &self.source {
            Source::Slice(bytes) => bytes,
            Source::Memory(bytes) => bytes.as_ref(),
        }
    }

    pub(super) fn decode(&mut self, encoding: &InputEncoding) {
        let Some(decoded) = Self::decoded(self.as_slice(), encoding) else {
            return;
        };
        self.source = Source::Memory(decoded);
    }

    fn decoded(raw: &[u8], encoding: &InputEncoding) -> Option<OwnedBytes> {
        match encoding {
            InputEncoding::Raw => None,
            InputEncoding::Auto => {
                let (enc, bom_len) = Encoding::for_bom(raw)?;
                Self::transcode(enc, &raw[bom_len..], bom_len == 0)
            }
            InputEncoding::Explicit(enc) => {
                if let Some((bom_enc, bom_len)) = Encoding::for_bom(raw) {
                    Self::transcode(bom_enc, &raw[bom_len..], false)
                } else {
                    Self::transcode(enc, raw, true)
                }
            }
        }
    }

    fn transcode(
        encoding: &'static Encoding,
        bytes: &[u8],
        keep_slice: bool,
    ) -> Option<OwnedBytes> {
        let (cow, _, _) = encoding.decode(bytes);
        match cow {
            Cow::Borrowed(_) if keep_slice => None,
            cow => Some(OwnedBytes::from_vec(cow.into_owned().into_bytes())),
        }
    }

    /// Rewrite NULs in the resident bytes. Returns the first file-absolute offset.
    pub(super) fn convert_nul(&mut self, term: u8) -> Option<u64> {
        match &mut self.source {
            Source::Slice(bytes) => {
                let idx = memchr(0, bytes)?;
                let mut copy = bytes.to_vec();
                for byte in &mut copy {
                    if *byte == 0 {
                        *byte = term;
                    }
                }
                self.source = Source::Memory(OwnedBytes::from_vec(copy));
                Some(u64::try_from(idx).unwrap_or(u64::MAX))
            }
            Source::Memory(bytes) => {
                Self::convert_bytes(bytes, term).map(|idx| u64::try_from(idx).unwrap_or(u64::MAX))
            }
        }
    }

    fn convert_bytes(bytes: &mut OwnedBytes, term: u8) -> Option<usize> {
        if let Some(slice) = bytes.as_mut_slice() {
            let idx = memchr(0, slice)?;
            for byte in &mut slice[idx..] {
                if *byte == 0 {
                    *byte = term;
                }
            }
            return Some(idx);
        }
        let mut copy = std::mem::replace(bytes, OwnedBytes::from_vec(Vec::new())).into_vec();
        let idx = memchr(0, &copy);
        if let Some(idx) = idx {
            for byte in &mut copy[idx..] {
                if *byte == 0 {
                    *byte = term;
                }
            }
        }
        *bytes = OwnedBytes::from_vec(copy);
        idx
    }
}

#[cfg(test)]
mod tests {
    use super::Bytes;
    use crate::search::input::{Input, Mention, Origin};
    use crate::search::options::{InputEncoding, Io};
    use std::borrow::Cow;

    #[test]
    fn auto_bom_utf16le_decodes() {
        let mut raw = vec![0xff, 0xfe];
        for unit in "needle\n".encode_utf16() {
            raw.extend_from_slice(&unit.to_le_bytes());
        }
        let input = Input::Bytes {
            origin: Origin::stream("t"),
            bytes: Cow::Owned(raw),
            mention: Mention::Explicit,
        };
        let mut loaded = Bytes::open(&input, Io::Sync).expect("open");
        loaded.decode(&InputEncoding::Auto);
        assert_eq!(loaded.as_slice(), b"needle\n");
    }

    #[test]
    fn explicit_utf8_keeps_resident_bytes() {
        let input = Input::Bytes {
            origin: Origin::stream("t"),
            bytes: Cow::Borrowed(b"needle\n"),
            mention: Mention::Explicit,
        };
        let mut loaded = Bytes::open(&input, Io::Sync).expect("open");
        let ptr = loaded.as_slice().as_ptr();
        loaded.decode(&InputEncoding::Explicit(encoding_rs::UTF_8));
        assert_eq!(loaded.as_slice().as_ptr(), ptr);
        assert_eq!(loaded.as_slice(), b"needle\n");
    }

    #[test]
    fn convert_nul_rewrites_and_reports_offset() {
        let input = Input::Bytes {
            origin: Origin::stream("t"),
            bytes: Cow::Borrowed(b"ab\0cd"),
            mention: Mention::Explicit,
        };
        let mut loaded = Bytes::open(&input, Io::Sync).expect("open");
        assert_eq!(loaded.convert_nul(b'\n'), Some(2));
        assert_eq!(loaded.as_slice(), b"ab\ncd");
    }
}
