use super::MAGIC;
use wincode::{SchemaWrite, config::DefaultConfig};

/// Stateless serializer that prepends the on-disk header to a wincode-encoded entry.
pub(super) struct Serializer;

/// Alias for [`Serializer`]; prefer this name at call sites for symmetry with [`HeaderDeserializer`].
///
/// [`HeaderDeserializer`]: crate::log::header::deserializer::HeaderDeserializer
pub(super) type HeaderSerializer = Serializer;

impl Serializer {
    /// Encodes `value` and wraps it in the on-disk header format:
    /// `[magic: 2B][crc32: 4B][entry_len: 4B][wincode payload]`.
    pub(super) fn serialize<T>(value: &T) -> anyhow::Result<Vec<u8>>
    where
        T: SchemaWrite<DefaultConfig, Src = T>,
    {
        let entry_bytes = wincode::serialize(value)?;
        let checksum = crc32fast::hash(&entry_bytes);
        let len = u32::try_from(entry_bytes.len())?;
        let mut bytes = Vec::<u8>::new();
        bytes.extend(MAGIC.to_le_bytes());
        bytes.extend(checksum.to_le_bytes());
        bytes.extend(len.to_le_bytes());
        bytes.extend(entry_bytes);
        Ok(bytes)
    }
}
