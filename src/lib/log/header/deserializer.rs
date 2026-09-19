use super::{HEADER_LEN, MAGIC};
use thiserror::Error;
use wincode::{SchemaRead, config::DefaultConfig};

/// Reasons an entry failed to parse from a byte slice.
#[derive(Debug, Error)]
pub(crate) enum CorruptionType {
    /// Slice is too short to contain a full header.
    #[error("slice too short for header and entry")]
    NotEnoughBytes,
    /// Magic bytes don't match the expected constant.
    #[error("missing magic bytes at entry boundary")]
    MagicBytesMismatch,
    /// CRC32 of the entry data doesn't match the stored checksum.
    #[error("checksum mismatch: entry data corrupted")]
    ChecksumMismatch,
    /// Entry data is present and checksums match, but wincode deserialization failed.
    #[error("failed to deserialize entry payload")]
    ParseError,
}

/// Stateless deserializer that validates and strips the on-disk header from a byte slice.
pub(super) struct Deserializer;

/// Alias for [`Deserializer`]; prefer this name at call sites for symmetry with [`HeaderSerializer`].
///
/// [`HeaderSerializer`]: crate::log::header::serializer::HeaderSerializer
pub(super) type HeaderDeserializer = Deserializer;

impl Deserializer {
    /// Parses a header-prefixed byte slice and returns the decoded value and total bytes consumed.
    ///
    /// Validates magic bytes and CRC32 before attempting deserialization. Returns the number of
    /// bytes consumed (`HEADER_LEN + entry_len`) so the caller can advance its read cursor.
    pub(super) fn deserialize<'de, T>(value: &'de [u8]) -> Result<(T, usize), CorruptionType>
    where
        T: SchemaRead<'de, DefaultConfig, Dst = T>,
    {
        if value.len() <= HEADER_LEN {
            return Err(CorruptionType::NotEnoughBytes);
        }
        let (magic_bytes, rest) = value
            .split_first_chunk::<2>()
            .ok_or(CorruptionType::NotEnoughBytes)?;
        if u16::from_le_bytes(*magic_bytes) != MAGIC {
            return Err(CorruptionType::MagicBytesMismatch);
        }
        let (checksum_bytes, rest) = rest
            .split_first_chunk::<4>()
            .ok_or(CorruptionType::NotEnoughBytes)?;
        let checksum = u32::from_le_bytes(*checksum_bytes);
        let (len_bytes, rest) = rest
            .split_first_chunk::<4>()
            .ok_or(CorruptionType::NotEnoughBytes)?;
        let len = usize::try_from(u32::from_le_bytes(*len_bytes))
            .map_err(|_| CorruptionType::NotEnoughBytes)?;
        let entry_bytes = rest.get(..len).ok_or(CorruptionType::NotEnoughBytes)?;
        if checksum != crc32fast::hash(entry_bytes) {
            return Err(CorruptionType::ChecksumMismatch);
        }
        let value: T = wincode::deserialize(entry_bytes).map_err(|_| CorruptionType::ParseError)?;
        let consumed = HEADER_LEN
            .checked_add(len)
            .ok_or(CorruptionType::NotEnoughBytes)?;
        Ok((value, consumed))
    }
}
