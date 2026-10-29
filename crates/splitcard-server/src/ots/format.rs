//! The OpenTimestamps proof format, with no I/O.
//!
//! Matches `python-opentimestamps` (`opentimestamps/core/{op,notary,
//! timestamp,serialize}.py`): a detached `.ots` file is the header magic, a
//! major version byte, the file hash op, the file digest and a timestamp tree.
//! A tree node is a list of items, each prefixed by `0xff` except the last; an
//! item is either `0x00` and an attestation, or an op and the subtree for its
//! result.
//!
//! Items keep the order they were parsed in, so
//! `serialize_file(&parse_file(bytes)?) == bytes` for every file this module
//! accepts. Like the reference client this accepts trees it cannot fully
//! evaluate: `sha1`, `ripemd160` and `keccak256` are parsed and written back
//! but never computed (only `sha2` is available), so branches below them are
//! skipped when computing commitments.

use sha2::{Digest, Sha256};
use thiserror::Error;

/// Bytes every detached timestamp file starts with.
pub const HEADER_MAGIC: &[u8; 31] =
    b"\x00OpenTimestamps\x00\x00Proof\x00\xbf\x89\xe2\xe8\x84\xe8\x92\x94";
/// The only supported major version.
pub const MAJOR_VERSION: u8 = 1;
/// Largest detached file this module parses.
pub const MAX_FILE_BYTES: usize = 64 * 1024;
/// Largest calendar response, as in the reference client.
pub const MAX_CALENDAR_RESPONSE_BYTES: usize = 10_000;
/// Deepest tree accepted: the reference client's recursion limit.
pub const MAX_DEPTH: usize = 256;
/// Longest message an op takes or returns.
pub const MAX_MSG_BYTES: usize = 4096;
/// Longest attestation payload.
pub const MAX_PAYLOAD_BYTES: usize = 8192;
/// Longest pending attestation URI.
pub const MAX_URI_BYTES: usize = 1000;

const ATTESTATION_TAG: u8 = 0x00;
const SEPARATOR: u8 = 0xff;
/// Pending attestation tag.
pub const PENDING_TAG: [u8; 8] = [0x83, 0xdf, 0xe3, 0x0d, 0x2e, 0xf9, 0x0c, 0x8e];
/// Bitcoin block header attestation tag.
pub const BITCOIN_TAG: [u8; 8] = [0x05, 0x88, 0x96, 0x0d, 0x73, 0xd7, 0x19, 0x01];
/// Characters a pending attestation URI may use.
const URI_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._/:";
const MAX_VARUINT_BYTES: usize = 9;

/// Why bytes are not a valid proof, or a proof operation failed.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OtsError {
    /// The input ended early.
    #[error("proof is truncated")]
    Truncated,
    /// The input is larger than allowed.
    #[error("proof is larger than {limit} bytes")]
    TooLarge {
        /// The limit in bytes.
        limit: usize,
    },
    /// The file does not start with the header magic.
    #[error("not an OpenTimestamps proof (bad header magic)")]
    BadMagic,
    /// The file's major version is not 1.
    #[error("unsupported proof major version {version}")]
    UnsupportedVersion {
        /// The version byte.
        version: u8,
    },
    /// The file hash op is not sha1, ripemd160 or sha256.
    #[error("unsupported file hash op 0x{tag:02x}")]
    UnknownFileHashOp {
        /// The op tag.
        tag: u8,
    },
    /// An op tag is unknown.
    #[error("unknown op tag 0x{tag:02x}")]
    UnknownOp {
        /// The op tag.
        tag: u8,
    },
    /// The tree is deeper than [`MAX_DEPTH`].
    #[error("timestamp tree is deeper than {MAX_DEPTH}")]
    TooDeep,
    /// A varuint is longer than 9 bytes or not minimally encoded.
    #[error("invalid varuint")]
    InvalidVarUint,
    /// A length-prefixed field is outside its allowed range.
    #[error("{field} length {len} is outside {min}..={max}")]
    BadLength {
        /// What was being read.
        field: &'static str,
        /// The length read.
        len: u64,
        /// Smallest allowed.
        min: usize,
        /// Largest allowed.
        max: usize,
    },
    /// A pending attestation URI uses a character outside the allowed set.
    #[error("pending attestation URI is invalid")]
    InvalidUri,
    /// A Bitcoin attestation height does not fit in 32 bits.
    #[error("block height is too large")]
    HeightTooLarge,
    /// A known attestation's payload has bytes after its fields.
    #[error("attestation payload has trailing bytes")]
    TrailingPayload,
    /// Bytes remain after the proof.
    #[error("proof has trailing bytes")]
    TrailingBytes,
    /// An op's input or result would be empty or longer than [`MAX_MSG_BYTES`].
    #[error("op message is empty or longer than {MAX_MSG_BYTES} bytes")]
    MessageLength,
    /// A path does not lead to a node in the tree.
    #[error("path does not exist in the timestamp tree")]
    PathNotFound,
    /// A timestamp to merge has nothing in it.
    #[error("empty timestamp")]
    Empty,
}

/// Hash op a detached file's digest was made with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileHashOp {
    /// SHA-1 (20 bytes).
    Sha1,
    /// RIPEMD-160 (20 bytes).
    Ripemd160,
    /// SHA-256 (32 bytes).
    Sha256,
}

impl FileHashOp {
    fn tag(self) -> u8 {
        match self {
            Self::Sha1 => 0x02,
            Self::Ripemd160 => 0x03,
            Self::Sha256 => 0x08,
        }
    }

    /// Digest length in bytes.
    #[must_use]
    pub fn digest_len(self) -> usize {
        match self {
            Self::Sha1 | Self::Ripemd160 => 20,
            Self::Sha256 => 32,
        }
    }
}

/// An edge in the tree: transforms the parent's message into the child's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// `0x02`: SHA-1. Parsed, never computed.
    Sha1,
    /// `0x03`: RIPEMD-160. Parsed, never computed.
    Ripemd160,
    /// `0x08`: SHA-256.
    Sha256,
    /// `0x67`: Keccak-256. Parsed, never computed.
    Keccak256,
    /// `0xf0`: append the bytes.
    Append(Vec<u8>),
    /// `0xf1`: prepend the bytes.
    Prepend(Vec<u8>),
    /// `0xf2`: reverse the message.
    Reverse,
    /// `0xf3`: lowercase hex of the message.
    Hexlify,
}

impl Op {
    fn tag(&self) -> u8 {
        match self {
            Self::Sha1 => 0x02,
            Self::Ripemd160 => 0x03,
            Self::Sha256 => 0x08,
            Self::Keccak256 => 0x67,
            Self::Append(_) => 0xf0,
            Self::Prepend(_) => 0xf1,
            Self::Reverse => 0xf2,
            Self::Hexlify => 0xf3,
        }
    }

    /// Applies the op, or `Ok(None)` for an op this module cannot compute.
    ///
    /// # Errors
    ///
    /// [`OtsError::MessageLength`] if the input or result is empty or too long.
    pub fn apply(&self, msg: &[u8]) -> Result<Option<Vec<u8>>, OtsError> {
        if msg.len() > MAX_MSG_BYTES {
            return Err(OtsError::MessageLength);
        }
        let result = match self {
            Self::Sha1 | Self::Ripemd160 | Self::Keccak256 => return Ok(None),
            Self::Sha256 => Sha256::digest(msg).to_vec(),
            Self::Append(suffix) => [msg, suffix].concat(),
            Self::Prepend(prefix) => [prefix.as_slice(), msg].concat(),
            Self::Reverse => msg.iter().rev().copied().collect(),
            Self::Hexlify => hex::encode(msg).into_bytes(),
        };
        if result.is_empty() || result.len() > MAX_MSG_BYTES {
            return Err(OtsError::MessageLength);
        }
        Ok(Some(result))
    }
}

/// A leaf claiming the node's message existed at some time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attestation {
    /// A calendar will later provide a complete attestation.
    Pending {
        /// The calendar's URI.
        uri: String,
    },
    /// The message is the merkle root of this Bitcoin block.
    Bitcoin {
        /// Block height.
        height: u32,
    },
    /// Any other attestation, kept byte-for-byte.
    Unknown {
        /// The 8-byte tag.
        tag: [u8; 8],
        /// The raw payload.
        payload: Vec<u8>,
    },
}

/// One item of a tree node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// An attestation to this node's message.
    Attestation(Attestation),
    /// An op and the subtree for its result.
    Op(Op, Timestamp),
}

/// A timestamp tree node. Parsed nodes always have at least one item.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Timestamp {
    /// Items in file order.
    pub items: Vec<Item>,
}

/// A detached `.ots` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedTimestampFile {
    /// How the stamped file was hashed.
    pub file_hash_op: FileHashOp,
    /// The stamped file's digest: the tree's root message.
    pub digest: Vec<u8>,
    /// The timestamp tree.
    pub timestamp: Timestamp,
}

/// A pending attestation and the commitment to ask its calendar about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRef {
    /// Item indices from the root to the node holding the attestation.
    pub path: Vec<usize>,
    /// The calendar URI.
    pub uri: String,
    /// The node's message.
    pub commitment: Vec<u8>,
}

/// Called with each node, its path and its message.
type Visitor<'v> = dyn FnMut(&Timestamp, &[usize], &[u8]) + 'v;

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], OtsError> {
        if len > self.bytes.len() {
            return Err(OtsError::Truncated);
        }
        let (head, rest) = self.bytes.split_at(len);
        self.bytes = rest;
        Ok(head)
    }

    fn byte(&mut self) -> Result<u8, OtsError> {
        Ok(self.take(1)?[0])
    }

    /// Unsigned LEB128, at most 9 bytes, minimally encoded so it round-trips.
    fn varuint(&mut self) -> Result<u64, OtsError> {
        let mut value: u64 = 0;
        for index in 0..MAX_VARUINT_BYTES {
            let byte = self.byte()?;
            value |= u64::from(byte & 0x7f) << (7 * index);
            if byte & 0x80 == 0 {
                if byte == 0 && index > 0 {
                    return Err(OtsError::InvalidVarUint);
                }
                return Ok(value);
            }
        }
        Err(OtsError::InvalidVarUint)
    }

    fn varbytes(
        &mut self,
        field: &'static str,
        min: usize,
        max: usize,
    ) -> Result<&'a [u8], OtsError> {
        let len = self.varuint()?;
        let bad = || OtsError::BadLength {
            field,
            len,
            min,
            max,
        };
        let len = usize::try_from(len).map_err(|_| bad())?;
        if len < min || len > max {
            return Err(bad());
        }
        self.take(len)
    }

    fn finish(&self, error: OtsError) -> Result<(), OtsError> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(error)
        }
    }
}

fn write_varuint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let low = (value & 0x7f).to_le_bytes()[0];
        value >>= 7;
        if value == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
}

fn write_varbytes(out: &mut Vec<u8>, bytes: &[u8]) {
    write_varuint(out, u64::try_from(bytes.len()).unwrap_or(u64::MAX));
    out.extend_from_slice(bytes);
}

fn parse_op(reader: &mut Reader<'_>, tag: u8) -> Result<Op, OtsError> {
    Ok(match tag {
        0x02 => Op::Sha1,
        0x03 => Op::Ripemd160,
        0x08 => Op::Sha256,
        0x67 => Op::Keccak256,
        0xf0 => Op::Append(
            reader
                .varbytes("append argument", 1, MAX_MSG_BYTES)?
                .to_vec(),
        ),
        0xf1 => Op::Prepend(
            reader
                .varbytes("prepend argument", 1, MAX_MSG_BYTES)?
                .to_vec(),
        ),
        0xf2 => Op::Reverse,
        0xf3 => Op::Hexlify,
        tag => return Err(OtsError::UnknownOp { tag }),
    })
}

fn parse_attestation(reader: &mut Reader<'_>) -> Result<Attestation, OtsError> {
    let mut tag = [0_u8; 8];
    tag.copy_from_slice(reader.take(8)?);
    let payload = reader.varbytes("attestation payload", 0, MAX_PAYLOAD_BYTES)?;
    let mut fields = Reader { bytes: payload };
    let attestation = match tag {
        PENDING_TAG => {
            let uri = fields.varbytes("pending URI", 0, MAX_URI_BYTES)?;
            if !uri.iter().all(|byte| URI_CHARS.contains(byte)) {
                return Err(OtsError::InvalidUri);
            }
            let uri = String::from_utf8(uri.to_vec()).map_err(|_| OtsError::InvalidUri)?;
            Attestation::Pending { uri }
        }
        BITCOIN_TAG => {
            let height = u32::try_from(fields.varuint()?).map_err(|_| OtsError::HeightTooLarge)?;
            Attestation::Bitcoin { height }
        }
        tag => {
            return Ok(Attestation::Unknown {
                tag,
                payload: payload.to_vec(),
            });
        }
    };
    fields.finish(OtsError::TrailingPayload)?;
    Ok(attestation)
}

fn parse_node(reader: &mut Reader<'_>, depth: usize) -> Result<Timestamp, OtsError> {
    if depth > MAX_DEPTH {
        return Err(OtsError::TooDeep);
    }
    let mut items = Vec::new();
    loop {
        let mut tag = reader.byte()?;
        let last = tag != SEPARATOR;
        if !last {
            tag = reader.byte()?;
        }
        items.push(if tag == ATTESTATION_TAG {
            Item::Attestation(parse_attestation(reader)?)
        } else {
            let op = parse_op(reader, tag)?;
            Item::Op(op, parse_node(reader, depth + 1)?)
        });
        if last {
            return Ok(Timestamp { items });
        }
    }
}

/// Parses a bare timestamp tree, such as a calendar response, whose root is
/// at depth `root_depth` (1 for a root, more when merging below a node).
///
/// # Errors
///
/// Any [`OtsError`] for malformed, oversized, too deep or trailing input.
pub fn parse_timestamp(bytes: &[u8], root_depth: usize) -> Result<Timestamp, OtsError> {
    if bytes.len() > MAX_CALENDAR_RESPONSE_BYTES {
        return Err(OtsError::TooLarge {
            limit: MAX_CALENDAR_RESPONSE_BYTES,
        });
    }
    let mut reader = Reader { bytes };
    let timestamp = parse_node(&mut reader, root_depth.max(1))?;
    reader.finish(OtsError::TrailingBytes)?;
    Ok(timestamp)
}

/// Parses a detached `.ots` file and checks every message it can compute.
///
/// # Errors
///
/// Any [`OtsError`] for malformed, oversized, too deep or trailing input.
pub fn parse_file(bytes: &[u8]) -> Result<DetachedTimestampFile, OtsError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(OtsError::TooLarge {
            limit: MAX_FILE_BYTES,
        });
    }
    let mut reader = Reader { bytes };
    if reader
        .take(HEADER_MAGIC.len())
        .map_err(|_| OtsError::BadMagic)?
        != HEADER_MAGIC
    {
        return Err(OtsError::BadMagic);
    }
    let version = reader.byte()?;
    if version != MAJOR_VERSION {
        return Err(OtsError::UnsupportedVersion { version });
    }
    let file_hash_op = match reader.byte()? {
        0x02 => FileHashOp::Sha1,
        0x03 => FileHashOp::Ripemd160,
        0x08 => FileHashOp::Sha256,
        tag => return Err(OtsError::UnknownFileHashOp { tag }),
    };
    let digest = reader.take(file_hash_op.digest_len())?.to_vec();
    let timestamp = parse_node(&mut reader, 1)?;
    reader.finish(OtsError::TrailingBytes)?;
    timestamp.check_messages(&digest)?;
    Ok(DetachedTimestampFile {
        file_hash_op,
        digest,
        timestamp,
    })
}

/// Writes a detached `.ots` file.
#[must_use]
pub fn serialize_file(file: &DetachedTimestampFile) -> Vec<u8> {
    let mut out = HEADER_MAGIC.to_vec();
    out.push(MAJOR_VERSION);
    out.push(file.file_hash_op.tag());
    out.extend_from_slice(&file.digest);
    file.timestamp.write(&mut out);
    out
}

impl Attestation {
    fn write(&self, out: &mut Vec<u8>) {
        out.push(ATTESTATION_TAG);
        let mut payload = Vec::new();
        let tag = match self {
            Self::Pending { uri } => {
                write_varbytes(&mut payload, uri.as_bytes());
                &PENDING_TAG
            }
            Self::Bitcoin { height } => {
                write_varuint(&mut payload, u64::from(*height));
                &BITCOIN_TAG
            }
            Self::Unknown { tag, payload: raw } => {
                payload.extend_from_slice(raw);
                tag
            }
        };
        out.extend_from_slice(tag);
        write_varbytes(out, &payload);
    }
}

impl Timestamp {
    /// A node with these items.
    #[must_use]
    pub fn new(items: Vec<Item>) -> Self {
        Self { items }
    }

    /// Writes the tree in the wire format.
    #[must_use]
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut Vec<u8>) {
        let count = self.items.len();
        for (index, item) in self.items.iter().enumerate() {
            if index + 1 < count {
                out.push(SEPARATOR);
            }
            match item {
                Item::Attestation(attestation) => attestation.write(out),
                Item::Op(op, child) => {
                    out.push(op.tag());
                    if let Op::Append(arg) | Op::Prepend(arg) = op {
                        write_varbytes(out, arg);
                    }
                    child.write(out);
                }
            }
        }
    }

    /// Number of node levels, 1 for a node with no op items.
    #[must_use]
    pub fn depth(&self) -> usize {
        1 + self
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Op(_, child) => Some(child.depth()),
                Item::Attestation(_) => None,
            })
            .max()
            .unwrap_or(0)
    }

    /// Calls `visit` with every node's path and message, skipping the
    /// subtrees below ops this module cannot compute.
    fn walk(
        &self,
        msg: &[u8],
        path: &mut Vec<usize>,
        visit: &mut Visitor<'_>,
    ) -> Result<(), OtsError> {
        visit(self, path, msg);
        for (index, item) in self.items.iter().enumerate() {
            if let Item::Op(op, child) = item
                && let Some(result) = op.apply(msg)?
            {
                path.push(index);
                child.walk(&result, path, visit)?;
                path.pop();
            }
        }
        Ok(())
    }

    /// Checks that every computable op's input and result are within limits,
    /// with `msg` as this node's message.
    ///
    /// # Errors
    ///
    /// [`OtsError::MessageLength`] for the first op that breaks a limit.
    pub fn check_messages(&self, msg: &[u8]) -> Result<(), OtsError> {
        self.walk(msg, &mut Vec::new(), &mut |_, _, _| {})
    }

    /// Every pending attestation with the commitment its calendar knows it
    /// by, in tree order, with `msg` as this node's message.
    ///
    /// # Errors
    ///
    /// [`OtsError::MessageLength`] if an op on the way breaks a limit.
    pub fn pending_attestations(&self, msg: &[u8]) -> Result<Vec<PendingRef>, OtsError> {
        let mut found = Vec::new();
        self.walk(msg, &mut Vec::new(), &mut |node, path, msg| {
            for item in &node.items {
                if let Item::Attestation(Attestation::Pending { uri }) = item {
                    found.push(PendingRef {
                        path: path.to_vec(),
                        uri: uri.clone(),
                        commitment: msg.to_vec(),
                    });
                }
            }
        })?;
        Ok(found)
    }

    /// Heights of every Bitcoin attestation anywhere in the tree.
    #[must_use]
    pub fn bitcoin_heights(&self) -> Vec<u32> {
        let mut heights = Vec::new();
        for item in &self.items {
            match item {
                Item::Attestation(Attestation::Bitcoin { height }) => heights.push(*height),
                Item::Attestation(_) => {}
                Item::Op(_, child) => heights.extend(child.bitcoin_heights()),
            }
        }
        heights
    }

    /// Adds every attestation and op of `other`, a timestamp for the same
    /// message, that this node lacks. Matching ops are merged recursively, so
    /// nothing either side had is lost; new items go after existing ones.
    pub fn merge(&mut self, other: Timestamp) {
        for item in other.items {
            match item {
                Item::Attestation(attestation) => {
                    let known = self.items.iter().any(
                        |existing| matches!(existing, Item::Attestation(a) if *a == attestation),
                    );
                    if !known {
                        self.items.push(Item::Attestation(attestation));
                    }
                }
                Item::Op(op, child) => {
                    let existing = self.items.iter_mut().find_map(|existing| match existing {
                        Item::Op(existing_op, existing_child) if *existing_op == op => {
                            Some(existing_child)
                        }
                        _ => None,
                    });
                    match existing {
                        Some(existing_child) => existing_child.merge(child),
                        None => self.items.push(Item::Op(op, child)),
                    }
                }
            }
        }
    }

    /// Merges `upgrade` into the node at `path` (as from [`PendingRef`]).
    ///
    /// # Errors
    ///
    /// [`OtsError::PathNotFound`] if `path` does not lead to a node,
    /// [`OtsError::Empty`] for an empty upgrade, [`OtsError::TooDeep`] if the
    /// result would exceed [`MAX_DEPTH`].
    pub fn merge_at(&mut self, path: &[usize], upgrade: Timestamp) -> Result<(), OtsError> {
        if upgrade.items.is_empty() {
            return Err(OtsError::Empty);
        }
        if path.len() + upgrade.depth() > MAX_DEPTH {
            return Err(OtsError::TooDeep);
        }
        let mut node = self;
        for &index in path {
            node = match node.items.get_mut(index) {
                Some(Item::Op(_, child)) => child,
                _ => return Err(OtsError::PathNotFound),
            };
        }
        node.merge(upgrade);
        Ok(())
    }
}
