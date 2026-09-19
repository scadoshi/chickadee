use super::{HEADER_LEN, MAGIC};
use thiserror::Error;
use wincode::{SchemaRead, config::DefaultConfig};

/// Why a byte slice did not yield an entry.
#[derive(Debug, Error)]
pub(crate) enum CorruptionType {
    /// Too short for the header plus the length it declares.
    #[error("slice too short for header and entry")]
    NotEnoughBytes,
    /// The first two bytes are not `MAGIC`.
    #[error("missing magic bytes at entry boundary")]
    MagicBytesMismatch,
    /// CRC32 of the payload differs from the stored one.
    #[error("checksum mismatch: entry data corrupted")]
    ChecksumMismatch,
    /// Header and checksum are fine but wincode rejected the payload.
    #[error("failed to deserialize entry payload")]
    ParseError,
}

/// Strips and validates the on-disk header from a byte slice.
pub(super) struct Deserializer;

/// Use this name at call sites; it pairs with [`HeaderSerializer`].
///
/// [`HeaderSerializer`]: crate::log::header::serializer::HeaderSerializer
pub(super) type HeaderDeserializer = Deserializer;

impl Deserializer {
    /// Parses a header-prefixed byte slice and returns the decoded value and the total
    /// bytes consumed.
    ///
    /// Validates the magic bytes and CRC32 before attempting deserialization. The number
    /// of bytes consumed is `HEADER_LEN + entry_len`, so the caller can advance its read
    /// cursor.
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
