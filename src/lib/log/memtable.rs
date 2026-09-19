use super::{
    entry::Entry,
    header::{reader::HeaderReader, writer::HeaderWriter},
    sstable::bloom_filter::BloomFilter,
};
use anyhow::Context;
use std::{
    collections::{BTreeMap, btree_map::Values},
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// Newest entry per key, sorted. Tracks its own byte size so the log knows when to flush.
#[derive(Debug)]
pub(crate) struct MemTable {
    inner: BTreeMap<String, Entry>,
    size: u64,
}

pub(super) const FLUSH_THRESHOLD_MB: u64 = 4;

impl MemTable {
    pub(crate) fn new() -> Self {
        let inner = BTreeMap::<String, Entry>::new();
        let size = 0;
        Self { inner, size }
    }

    /// Replays `file` from the start.
    pub(crate) fn from_file(file: &mut File) -> anyhow::Result<Self> {
        let mut memtable = Self::new();
        file.seek(SeekFrom::Start(0))?;
        while let Ok(Some(entry)) = file.header_read_next() {
            memtable.process(entry)?;
        }
        Ok(memtable)
    }

    /// An estimate: key length plus serialized entry length, not the `BTreeMap`'s footprint.
    pub(crate) fn size(&self) -> u64 {
        self.size
    }

    /// Tombstones are stored like any other entry. Returns whatever the key held before.
    pub(crate) fn process(&mut self, entry: Entry) -> anyhow::Result<Option<Entry>> {
        if let Some(previous) = self.inner.get(entry.key()) {
            self.size = self.size.saturating_sub(Self::entry_size(previous)?);
        }
        self.size = self.size.saturating_add(Self::entry_size(&entry)?);
        Ok(self.inner.insert(entry.key().to_owned(), entry))
    }

    fn entry_size(entry: &Entry) -> anyhow::Result<u64> {
        let key_len = u64::try_from(entry.key().len())?;
        let payload_len = u64::try_from(wincode::serialize(entry)?.len())?;
        Ok(key_len.saturating_add(payload_len))
    }

    pub(crate) fn should_flush(&self) -> bool {
        self.size() > FLUSH_THRESHOLD_MB * (1 << 20)
    }

    /// Writes a new timestamped `SSTable` under `path`, then clears the memtable.
    pub(crate) fn flush_to(&mut self, mut path: PathBuf) -> anyhow::Result<()> {
        if self.is_empty() {
            return Ok(());
        }
        std::fs::create_dir_all(&path)?;
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros();
        path.push(format!("{ts:020}.sst"));
        let mut file = OpenOptions::new()
            .truncate(true)
            .create(true)
            .write(true)
            .open(path)?;
        // 10 bits per key is ideal for k=7
        let bit_count = self
            .len()
            .checked_mul(10)
            .context("bloom filter bit count overflow")?;
        let mut bloom_filter = BloomFilter::new(bit_count);
        for entry in self.values() {
            file.header_write(entry)?;
            bloom_filter.insert(entry.key().as_bytes());
        }
        file.write_all(&bloom_filter)?;
        file.write_all(&u32::try_from(bit_count)?.to_le_bytes())?;
        file.sync_all()?;
        self.clear();
        Ok(())
    }

    pub(crate) fn clear(&mut self) {
        self.size = 0;
        self.inner.clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.inner.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.size == 0 && self.inner.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn contains_key(&self, key: impl AsRef<str>) -> bool {
        self.inner.contains_key(key.as_ref())
    }

    pub(crate) fn get(&self, key: impl AsRef<str>) -> Option<&Entry> {
        self.inner.get(key.as_ref())
    }

    pub(crate) fn values(&self) -> Values<'_, String, Entry> {
        self.inner.values()
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::arithmetic_side_effects
    )]
    use super::*;
    use crate::log::{Log, header::writer::HeaderWriter};
    use std::{fs::read_dir, io::Read};

    #[test]
    fn from_file_empty() {
        let mut file = tempfile::tempfile().unwrap();
        let memtable = MemTable::from_file(&mut file).unwrap();
        assert!(memtable.is_empty());
    }

    #[test]
    fn from_file_single_set() {
        let mut file = tempfile::tempfile().unwrap();
        let set = Entry::set("a", "1");
        file.header_write(&set).unwrap();
        let memtable = MemTable::from_file(&mut file).unwrap();
        assert_eq!(memtable.len(), 1);
        assert!(memtable.contains_key(set.key()));
    }

    #[test]
    fn from_file_set_then_delete_stores_tombstone() {
        let mut file = tempfile::tempfile().unwrap();
        let set = Entry::set("a", "1");
        let delete = Entry::delete("a");
        file.header_write(&set).unwrap();
        file.header_write(&delete).unwrap();
        let memtable = MemTable::from_file(&mut file).unwrap();
        assert_eq!(memtable.len(), 1);
        assert!(memtable.contains_key("a"));
        assert!(matches!(memtable.get("a").unwrap(), Entry::Delete { .. }));
    }

    #[test]
    fn from_file_multiple_keys() {
        let mut file = tempfile::tempfile().unwrap();
        let a = Entry::set("a", "1");
        let b = Entry::set("b", "2");
        let c = Entry::set("c", "3");
        file.header_write(&a).unwrap();
        file.header_write(&b).unwrap();
        file.header_write(&c).unwrap();
        let memtable = MemTable::from_file(&mut file).unwrap();
        assert_eq!(memtable.len(), 3);
        assert!(memtable.contains_key(a.key()));
        assert!(memtable.contains_key(b.key()));
        assert!(memtable.contains_key(c.key()));
    }

    #[test]
    fn from_file_delete_stores_tombstone() {
        let mut file = tempfile::tempfile().unwrap();
        let delete = Entry::delete("a");
        file.header_write(&delete).unwrap();
        let memtable = MemTable::from_file(&mut file).unwrap();
        assert_eq!(memtable.len(), 1);
        assert!(memtable.contains_key("a"));
        assert!(matches!(memtable.get("a").unwrap(), Entry::Delete { .. }));
    }

    #[test]
    fn process_set_increments_size() {
        let mut file = tempfile::tempfile().unwrap();
        let mut memtable = MemTable::from_file(&mut file).unwrap();
        let set = Entry::set("a", "1");
        memtable.process(set).unwrap();
        assert!(memtable.size() > 0);
    }

    #[test]
    fn process_delete_replaces_set_with_smaller_tombstone() {
        let mut file = tempfile::tempfile().unwrap();
        let mut memtable = MemTable::from_file(&mut file).unwrap();
        let set = Entry::set("a", "1");
        let delete = Entry::delete("a");
        memtable.process(set).unwrap();
        let size = memtable.size();
        memtable.process(delete).unwrap();
        // tombstone is smaller than set (no value), but nonzero
        assert!(size > memtable.size());
        assert!(memtable.size() > 0);
    }

    #[test]
    fn process_set_overwrite_does_not_double_count_size() {
        let mut file = tempfile::tempfile().unwrap();
        let mut memtable = MemTable::from_file(&mut file).unwrap();
        let set = Entry::set("a", "1");
        memtable.process(set.clone()).unwrap();
        let size_after_set = memtable.size();
        memtable.process(set).unwrap();
        assert_eq!(size_after_set, memtable.size());
    }

    #[test]
    fn process_delete_nonexistent_key_stores_tombstone() {
        let mut file = tempfile::tempfile().unwrap();
        let mut memtable = MemTable::from_file(&mut file).unwrap();
        let delete = Entry::delete("a");
        memtable.process(delete).unwrap();
        assert_eq!(memtable.len(), 1);
        assert!(memtable.size() > 0);
    }

    #[test]
    fn clear_resets_size_and_entries() {
        let mut file = tempfile::tempfile().unwrap();
        let mut memtable = MemTable::from_file(&mut file).unwrap();
        let set = Entry::set("a", "1");
        memtable.process(set).unwrap();
        memtable.clear();
        assert!(memtable.is_empty());
        assert_eq!(memtable.size(), 0);
    }

    #[test]
    fn get_returns_some_for_existing_key() {
        let mut file = tempfile::tempfile().unwrap();
        let mut memtable = MemTable::from_file(&mut file).unwrap();
        let set = Entry::set("a", "1");
        memtable.process(set.clone()).unwrap();
        let result = memtable.get("a").unwrap();
        assert_eq!(set.key(), result.key());
        assert_eq!(set.value(), result.value());
    }

    #[test]
    fn get_returns_none_for_absent_key() {
        let mut file = tempfile::tempfile().unwrap();
        let memtable = MemTable::from_file(&mut file).unwrap();
        assert!(memtable.get("a").is_none());
    }

    #[test]
    fn contains_key_true_for_existing() {
        let mut file = tempfile::tempfile().unwrap();
        let mut memtable = MemTable::from_file(&mut file).unwrap();
        let set = Entry::set("a", "1");
        memtable.process(set.clone()).unwrap();
        assert!(memtable.contains_key(set.key()));
    }

    #[test]
    fn contains_key_false_for_absent() {
        let mut file = tempfile::tempfile().unwrap();
        let memtable = MemTable::from_file(&mut file).unwrap();
        assert!(!memtable.contains_key("a"));
    }

    #[test]
    fn len_reflects_unique_key_count() {
        let mut file = tempfile::tempfile().unwrap();
        let mut memtable = MemTable::from_file(&mut file).unwrap();
        let set_a = Entry::set("a", "1");
        let set_b = Entry::set("b", "2");
        memtable.process(set_a.clone()).unwrap();
        memtable.process(set_b).unwrap();
        let len = memtable.len();
        memtable.process(set_a).unwrap();
        assert_eq!(len, memtable.len());
    }

    #[test]
    fn values_iterates_in_sorted_key_order() {
        let mut file = tempfile::tempfile().unwrap();
        let mut memtable = MemTable::from_file(&mut file).unwrap();
        let set1 = Entry::set("a", "1");
        let set2 = Entry::set("b", "2");
        let set3 = Entry::set("c", "3");
        memtable.process(set3.clone()).unwrap();
        memtable.process(set2.clone()).unwrap();
        memtable.process(set1.clone()).unwrap();
        let expected = vec![&set1, &set2, &set3];
        let result: Vec<_> = memtable.values().collect();
        assert_eq!(expected, result);
    }

    fn temp_log() -> (tempfile::TempDir, Log) {
        let dir = tempfile::tempdir().unwrap();
        let log = Log::new(
            dir.path(),
            dir.path().join("memtable"),
            dir.path().join("sstables"),
            true,
        )
        .unwrap();
        (dir, log)
    }

    #[test]
    fn flush_writes_bloomfilter_footer_to_sstable() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.write(Entry::set("b", "2")).unwrap();
        log.write(Entry::set("c", "3")).unwrap();
        log.flush().unwrap();
        let sstable_path = read_dir(log.sstables_path)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        let mut file = File::open(sstable_path).unwrap();
        let (bit_count, byte_count) = {
            file.seek(SeekFrom::End(-4)).unwrap();
            let mut bytes = Vec::<u8>::new();
            file.read_to_end(&mut bytes).unwrap();
            let bit_count = u32::from_le_bytes(bytes.as_slice().try_into().unwrap()) as usize;
            let byte_count = bit_count.div_ceil(8);
            file.seek(SeekFrom::End(-(byte_count as i64) - 4)).unwrap();
            (bit_count, byte_count)
        };
        // three keys at ten bits each
        assert_eq!(bit_count, 30);
        // 30 bits rounds up to 4 bytes
        assert_eq!(byte_count, 4);
    }

    #[test]
    fn bloomfilter_reports_present_for_inserted_key() {
        let (_dir, mut log) = temp_log();
        let set = Entry::set("a", "1");
        log.write(set.clone()).unwrap();
        log.flush().unwrap();
        let sstable_path = read_dir(log.sstables_path)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        let mut file = File::open(sstable_path).unwrap();
        let (bit_count, byte_count) = {
            file.seek(SeekFrom::End(-4)).unwrap();
            let mut bytes = Vec::<u8>::new();
            file.read_to_end(&mut bytes).unwrap();
            let bit_count = u32::from_le_bytes(bytes.as_slice().try_into().unwrap()) as usize;
            let byte_count = bit_count.div_ceil(8);
            file.seek(SeekFrom::End(-(byte_count as i64) - 4)).unwrap();
            (bit_count, byte_count)
        };
        let bloomfilter: Vec<u8> = {
            let mut bytes = Vec::<u8>::new();
            file.read_to_end(&mut bytes).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            bytes.into_iter().take(byte_count).collect()
        };
        let hash1 = xxh3::hash64_with_seed(set.key().as_bytes(), 0);
        let hash2 = xxh3::hash64_with_seed(set.key().as_bytes(), 1);
        for i in 0..7 {
            let pos =
                (hash1.wrapping_add((i as u64).wrapping_mul(hash2)) % bit_count as u64) as usize;
            assert_ne!(bloomfilter[pos / 8] & 1 << (pos % 8), 0);
        }
    }

    #[test]
    fn bloomfilter_reports_absent_for_missing_key() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        let sstable_path = read_dir(log.sstables_path)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        let mut file = File::open(sstable_path).unwrap();
        let (bit_count, byte_count) = {
            file.seek(SeekFrom::End(-4)).unwrap();
            let mut bytes = Vec::<u8>::new();
            file.read_to_end(&mut bytes).unwrap();
            let bit_count = u32::from_le_bytes(bytes.as_slice().try_into().unwrap()) as usize;
            let byte_count = bit_count.div_ceil(8);
            file.seek(SeekFrom::End(-(byte_count as i64) - 4)).unwrap();
            (bit_count, byte_count)
        };
        let bloomfilter: Vec<u8> = {
            let mut bytes = Vec::<u8>::new();
            file.read_to_end(&mut bytes).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            bytes.into_iter().take(byte_count).collect()
        };
        let missing_key = "z";
        let hash1 = xxh3::hash64_with_seed(missing_key.as_bytes(), 0);
        let hash2 = xxh3::hash64_with_seed(missing_key.as_bytes(), 1);
        assert!((0..7).any(|i| {
            let pos =
                (hash1.wrapping_add((i as u64).wrapping_mul(hash2)) % bit_count as u64) as usize;
            bloomfilter[pos / 8] & 1 << (pos % 8) == 0
        }));
    }
}
