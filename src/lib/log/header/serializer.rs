use super::MAGIC;
use wincode::{SchemaWrite, config::DefaultConfig};

/// Prepends the on-disk header to a wincode-encoded value.
pub(super) struct Serializer;

/// Use this name at call sites; it pairs with [`HeaderDeserializer`].
///
/// [`HeaderDeserializer`]: crate::log::header::deserializer::HeaderDeserializer
pub(super) type HeaderSerializer = Serializer;

impl Serializer {
    /// Encodes `value` as `[magic: 2B][crc32: 4B][entry_len: 4B][wincode payload]`.
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
