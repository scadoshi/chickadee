pub(crate) mod deserializer;
pub(crate) mod reader;
pub(crate) mod serializer;
pub(crate) mod writer;

/// Size of the entry header in bytes: magic (2) + crc32 (4) + `entry_len` (4).
pub(super) const HEADER_LEN: usize = 10;
/// Magic bytes written at the start of every entry, used to locate entry boundaries.
///
/// Serialized little-endian, so the literal is byte-swapped relative to what lands on
/// disk: 0x4443 writes `[0x43, 0x44]`, which is "CD" in a hex dump. That readability is
/// the whole point of a magic number, since it is what lets you eyeball entry boundaries
/// in a corrupt file.
pub(super) const MAGIC: u16 = 0x4443;

#[cfg(test)]
mod tests {
    #![allow(clippy::as_conversions, clippy::cast_possible_truncation)]
    use super::{
        MAGIC,
        deserializer::{CorruptionType, HeaderDeserializer},
        reader::HeaderReader,
        serializer::HeaderSerializer,
        writer::HeaderWriter,
    };
    use crate::log::entry::Entry;
    use std::io::{Seek, SeekFrom, Write};

    // --- Round-trips ---

    #[test]
    fn write_then_read_wal_set_round_trips() {
        let mut file = tempfile::tempfile().unwrap();
        let entry = Entry::set("hello", "world");
        file.header_write(&entry).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let result: Entry = file.header_read_next().unwrap().unwrap();
        assert_eq!(result, entry);
    }

    #[test]
    fn write_then_read_wal_delete_round_trips() {
        let mut file = tempfile::tempfile().unwrap();
        let entry = Entry::delete("mykey");
        file.header_write(&entry).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let result: Entry = file.header_read_next().unwrap().unwrap();
        assert_eq!(result, entry);
    }

    // --- Corruption recovery ---

    #[test]
    fn read_next_skips_garbage_prefix_and_finds_entry() {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&[0xFF; 5]).unwrap();
        let entry = Entry::set("key", "value");
        file.header_write(&entry).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let result: Entry = file.header_read_next().unwrap().unwrap();
        assert_eq!(result, entry);
    }

    #[test]
    fn read_next_returns_none_on_empty_file() {
        let mut file = tempfile::tempfile().unwrap();
        let result: Option<Entry> = file.header_read_next().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn read_next_returns_none_when_cursor_at_eof() {
        let mut file = tempfile::tempfile().unwrap();
        file.header_write(&Entry::set("a", "1")).unwrap();
        // cursor is already at EOF after header_write
        let result: Option<Entry> = file.header_read_next().unwrap();
        assert!(result.is_none());
    }

    // --- has_at_least_one ---

    #[test]
    fn has_at_least_one_returns_true_with_entry() {
        let mut file = tempfile::tempfile().unwrap();
        file.header_write(&Entry::set("a", "1")).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let pos_before = file.stream_position().unwrap();
        let result = HeaderReader::<Entry>::header_has_at_least_one(&mut file).unwrap();
        assert!(result);
        assert_eq!(file.stream_position().unwrap(), pos_before);
    }

    #[test]
    fn has_at_least_one_returns_false_on_empty_file() {
        let mut file = tempfile::tempfile().unwrap();
        let result = HeaderReader::<Entry>::header_has_at_least_one(&mut file).unwrap();
        assert!(!result);
    }

    // --- CorruptionType variants ---

    #[test]
    fn deserialize_err_not_enough_bytes() {
        let short = [0u8; 5];
        let result = HeaderDeserializer::deserialize::<Entry>(&short);
        assert!(matches!(result, Err(CorruptionType::NotEnoughBytes)));
    }

    #[test]
    fn deserialize_err_magic_mismatch() {
        let entry = Entry::set("a", "1");
        let mut bytes = HeaderSerializer::serialize(&entry).unwrap();
        bytes[0] = 0x00;
        bytes[1] = 0x00;
        let result = HeaderDeserializer::deserialize::<Entry>(&bytes);
        assert!(matches!(result, Err(CorruptionType::MagicBytesMismatch)));
    }

    #[test]
    fn deserialize_err_checksum_mismatch() {
        let entry = Entry::set("a", "1");
        let mut bytes = HeaderSerializer::serialize(&entry).unwrap();
        bytes[2] = 0x00;
        bytes[3] = 0x00;
        bytes[4] = 0x00;
        bytes[5] = 0x00;
        let result = HeaderDeserializer::deserialize::<Entry>(&bytes);
        assert!(matches!(result, Err(CorruptionType::ChecksumMismatch)));
    }

    #[test]
    fn deserialize_err_parse_error() {
        let garbage = vec![0xFFu8; 20];
        let checksum = crc32fast::hash(&garbage);
        let mut bytes = Vec::new();
        bytes.extend(MAGIC.to_le_bytes());
        bytes.extend(checksum.to_le_bytes());
        bytes.extend((garbage.len() as u32).to_le_bytes());
        bytes.extend(&garbage);
        let result = HeaderDeserializer::deserialize::<Entry>(&bytes);
        assert!(matches!(result, Err(CorruptionType::ParseError)));
    }
}
