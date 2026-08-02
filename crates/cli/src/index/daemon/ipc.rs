//! Daemon IPC: request/response wire format and client handle.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;

use interprocess::local_socket::traits::{ListenerExt, Stream as _};
use interprocess::local_socket::{GenericNamespaced, Listener, ListenerOptions, Stream, ToNsName};
use sift_core::SnapshotId;

use super::{DaemonError, DaemonOrchestrator, Event};

/// IPC request sent to the index daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonRequest {
    /// Rel-paths to index. Empty vec = full corpus.
    Index(Vec<PathBuf>),
    /// Validate that an opened snapshot is the daemon's committed read version.
    ValidateSnapshot(SnapshotId),
}

impl DaemonRequest {
    const INDEX_OPCODE: u8 = 0x02;
    const VALIDATE_SNAPSHOT_OPCODE: u8 = 0x03;

    #[must_use]
    pub const fn index(paths: Vec<PathBuf>) -> Self {
        Self::Index(paths)
    }

    #[must_use]
    pub const fn validate_snapshot(id: SnapshotId) -> Self {
        Self::ValidateSnapshot(id)
    }

    /// Encode this operation for IPC.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    pub fn encode(&self, writer: &mut impl Write) -> io::Result<()> {
        match self {
            Self::Index(paths) => {
                writer.write_all(&[Self::INDEX_OPCODE])?;
                for path in paths {
                    let line = path.to_string_lossy();
                    writer.write_all(line.as_bytes())?;
                    writer.write_all(b"\n")?;
                }
                writer.write_all(b"\n")?;
            }
            Self::ValidateSnapshot(id) => {
                writer.write_all(&[Self::VALIDATE_SNAPSHOT_OPCODE])?;
                writer.write_all(id.as_str().as_bytes())?;
                writer.write_all(b"\n")?;
            }
        }
        writer.flush()
    }

    /// Decode a daemon operation from IPC.
    ///
    /// # Errors
    ///
    /// Returns an error if the payload is malformed.
    pub fn decode(mut reader: impl Read) -> io::Result<Self> {
        let mut opcode = [0_u8; 1];
        reader.read_exact(&mut opcode)?;
        match opcode[0] {
            Self::INDEX_OPCODE => {
                let mut paths = Vec::new();
                loop {
                    let mut buf = Vec::new();
                    loop {
                        let mut byte = [0_u8; 1];
                        let n = reader.read(&mut byte)?;
                        if n == 0 || byte[0] == b'\n' {
                            break;
                        }
                        buf.push(byte[0]);
                    }
                    if buf.is_empty() {
                        break;
                    }
                    let line = String::from_utf8(buf).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidData, "index path is not valid utf-8")
                    })?;
                    paths.push(PathBuf::from(line));
                }
                Ok(Self::Index(paths))
            }
            Self::VALIDATE_SNAPSHOT_OPCODE => {
                let mut buf = Vec::new();
                loop {
                    let mut byte = [0_u8; 1];
                    let n = reader.read(&mut byte)?;
                    if n == 0 || byte[0] == b'\n' {
                        break;
                    }
                    buf.push(byte[0]);
                }
                let id = String::from_utf8(buf).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "snapshot id is not valid utf-8")
                })?;
                Ok(Self::ValidateSnapshot(SnapshotId::new(id)))
            }
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown daemon opcode: {other}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonResponse {
    Accepted,
    SnapshotValid,
    SnapshotBehind,
    Error(String),
}

impl DaemonResponse {
    const ACCEPTED: u8 = 0x00;
    const SNAPSHOT_VALID: u8 = 0x01;
    const SNAPSHOT_BEHIND: u8 = 0x02;
    const ERROR: u8 = 0xff;

    pub(super) fn encode(&self, writer: &mut impl Write) -> io::Result<()> {
        match self {
            Self::Accepted => writer.write_all(&[Self::ACCEPTED])?,
            Self::SnapshotValid => writer.write_all(&[Self::SNAPSHOT_VALID])?,
            Self::SnapshotBehind => writer.write_all(&[Self::SNAPSHOT_BEHIND])?,
            Self::Error(message) => {
                writer.write_all(&[Self::ERROR])?;
                writer.write_all(message.as_bytes())?;
                writer.write_all(b"\n")?;
            }
        }
        writer.flush()
    }

    pub(super) fn decode(mut reader: impl Read) -> io::Result<Self> {
        let mut opcode = [0_u8; 1];
        reader.read_exact(&mut opcode)?;
        match opcode[0] {
            Self::ACCEPTED => Ok(Self::Accepted),
            Self::SNAPSHOT_VALID => Ok(Self::SnapshotValid),
            Self::SNAPSHOT_BEHIND => Ok(Self::SnapshotBehind),
            Self::ERROR => {
                let mut buf = Vec::new();
                loop {
                    let mut byte = [0_u8; 1];
                    let n = reader.read(&mut byte)?;
                    if n == 0 || byte[0] == b'\n' {
                        break;
                    }
                    buf.push(byte[0]);
                }
                let message = String::from_utf8(buf).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "daemon error is not valid utf-8",
                    )
                })?;
                Ok(Self::Error(message))
            }
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown daemon response: {other}"),
            )),
        }
    }
}

pub(super) struct ClientRequest {
    pub(super) request: DaemonRequest,
    pub(super) response: mpsc::Sender<DaemonResponse>,
}

/// Handle to the index daemon for a `.sift` store directory.
#[derive(Debug, Clone)]
pub struct Daemon {
    pub(crate) sift_dir: PathBuf,
}

impl Daemon {
    /// Client handle for the sift CLI (search / index commands).
    #[must_use]
    pub const fn new(sift_dir: PathBuf) -> Self {
        Self { sift_dir }
    }

    /// Queue index work. Empty `paths` = full corpus reconcile.
    ///
    /// # Errors
    ///
    /// Propagates spawn and IPC failures.
    pub fn index(&self, paths: Vec<PathBuf>) -> Result<(), DaemonError> {
        self.invoke(&DaemonRequest::index(paths))
            .and_then(|response| match response {
                DaemonResponse::Accepted => Ok(()),
                DaemonResponse::Error(message) => Err(DaemonError::message(message)),
                DaemonResponse::SnapshotValid | DaemonResponse::SnapshotBehind => {
                    Err(DaemonError::message("daemon returned unexpected response"))
                }
            })
    }

    /// Check whether this exact snapshot is a valid daemon read version.
    ///
    /// # Errors
    ///
    /// Propagates spawn and IPC failures.
    pub fn validate_snapshot(&self, id: &SnapshotId) -> Result<bool, DaemonError> {
        self.invoke(&DaemonRequest::validate_snapshot(id.clone()))
            .and_then(|response| match response {
                DaemonResponse::SnapshotValid => Ok(true),
                DaemonResponse::SnapshotBehind => Ok(false),
                DaemonResponse::Error(message) => Err(DaemonError::message(message)),
                DaemonResponse::Accepted => {
                    Err(DaemonError::message("daemon returned unexpected response"))
                }
            })
    }

    pub(crate) fn reachable(&self) -> Result<bool, DaemonError> {
        let name = self.ipc_name()?;
        Ok(Stream::connect(name).is_ok())
    }

    pub(crate) fn ipc_name(
        &self,
    ) -> Result<interprocess::local_socket::Name<'static>, DaemonError> {
        let canonical = self
            .sift_dir
            .canonicalize()
            .unwrap_or_else(|_| self.sift_dir.clone());
        let mut hasher = DefaultHasher::new();
        canonical.hash(&mut hasher);
        format!("sift-{:016x}", hasher.finish())
            .to_ns_name::<GenericNamespaced>()
            .map_err(DaemonError::Io)
    }

    pub(super) fn bind_listener(&self) -> Result<Listener, DaemonError> {
        ListenerOptions::new()
            .name(self.ipc_name()?)
            .try_overwrite(true)
            .create_sync()
            .map_err(DaemonError::Io)
    }

    /// Accept IPC connections and forward each request as [`Event::Client`].
    pub(super) fn accept_clients(listener: &Listener, events: &mpsc::Sender<Event>) {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let request = match DaemonRequest::decode(&mut stream) {
                Ok(op) => op,
                Err(e) => {
                    let _ = DaemonResponse::Error(e.to_string()).encode(&mut stream);
                    eprintln!("sift-daemon: ipc decode failed: {e}");
                    continue;
                }
            };
            let (response_tx, response_rx) = mpsc::channel();
            if events
                .send(Event::Client(ClientRequest {
                    request,
                    response: response_tx,
                }))
                .is_err()
            {
                let _ = DaemonResponse::Error("daemon stopped accepting requests".into())
                    .encode(&mut stream);
                break;
            }
            match response_rx.recv() {
                Ok(response) => {
                    let _ = response.encode(&mut stream);
                }
                Err(_) => {
                    let _ = DaemonResponse::Error("daemon stopped accepting requests".into())
                        .encode(&mut stream);
                }
            }
        }
    }

    fn invoke(&self, request: &DaemonRequest) -> Result<DaemonResponse, DaemonError> {
        DaemonOrchestrator::new(self.sift_dir.clone(), None).start()?;
        let mut stream = Stream::connect(self.ipc_name()?).map_err(DaemonError::Io)?;
        request.encode(&mut stream)?;
        DaemonResponse::decode(&mut stream).map_err(DaemonError::Io)
    }

    /// Resolve the `sift-daemon` binary for spawn and integration tests.
    ///
    /// # Errors
    ///
    /// Returns an IO error if the current executable path cannot be read.
    pub fn executable() -> Result<PathBuf, DaemonError> {
        DaemonOrchestrator::executable()
    }
}
