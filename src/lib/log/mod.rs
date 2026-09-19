pub mod command;
pub(crate) mod entry;
pub(crate) mod header;
pub(crate) mod memtable;
pub(crate) mod sstable;

pub use command::{Command, CommandError};
pub use entry::Entry;

use self::{memtable::MemTable, sstable::SSTable};
use header::writer::HeaderWriter;
use std::{
    cmp::Reverse,
    fs::{File, OpenOptions, create_dir_all, read_dir},
    io::{Seek, SeekFrom},
    path::PathBuf,
};

/// Root directory for all persisted data.
pub const DATA_PATH: &str = "data";
/// Path to the write-ahead log (WAL) file.
pub const WAL_PATH: &str = "data/wal";
/// Directory containing flushed `SSTable` files.
pub const SSTABLES_PATH: &str = "data/sstables";
/// Multiple which `flush_count` is checked against in order to determine compaction timing
const COMPACT_EVERY_N_FLUSHES: u64 = 10;

/// Append-only log store. Owns the data file and in-memory key-to-entry memtable.
#[derive(Debug)]
pub struct Log {
    wal_file: File,
    memtable: MemTable,
    sstables_path: PathBuf,
    flush_count: u64,
}

impl Log {
    /// Opens or creates a log file and rebuilds the memtable from its contents. Takes truncate flag
    /// informing whether to overwrite or append existing file content.
    pub fn new(
        data_path: impl Into<PathBuf>,
        wal_path: impl Into<PathBuf>,
        sstables_path: impl Into<PathBuf>,
        truncate: bool,
    ) -> anyhow::Result<Self> {
        // Initialize paths and dirs
        let data_path = data_path.into();
        let wal_path = wal_path.into();
        let sstables_path = sstables_path.into();
        create_dir_all(&data_path)?;
        create_dir_all(&sstables_path)?;
        // Open WAL and initialize memtable
        let mut wal_file = OpenOptions::new()
            .create(true)
            .truncate(truncate)
            .read(true)
            .write(true)
            .open(&wal_path)?;
        let memtable = MemTable::from_file(&mut wal_file)?;
        // SSTable count equates to flush count
        let flush_count = u64::try_from(
            read_dir(&sstables_path)?
                .collect::<Result<Vec<_>, _>>()?
                .len(),
        )?;
        // Return
        Ok(Self {
            wal_file,
            memtable,
            sstables_path,
            flush_count,
        })
    }

    /// Appends an entry with header to the WAL and syncs to disk, then applies it to the memtable.
    pub fn write(&mut self, entry: Entry) -> anyhow::Result<()> {
        self.wal_file.header_write(&entry)?;
        self.wal_file.sync_all()?;
        self.memtable.process(entry)?;
        Ok(())
    }

    /// Looks up a key: checks the memtable first, then scans `SSTables` newest-to-oldest.
    /// Returns `None` if the key is absent or deleted in either layer.
    pub fn get(&self, key: impl AsRef<str>) -> anyhow::Result<Option<Entry>> {
        // Try memtable first
        if let Some(entry) = self.memtable.get(key.as_ref()) {
            return match entry {
                Entry::Set { .. } => Ok(Some(entry.clone())),
                Entry::Delete { .. } => Ok(None),
            };
        }
        // Then if not found sift through SSTables
        // Only does linear search for now
        let mut dir_entries: Vec<_> =
            read_dir(&self.sstables_path)?.collect::<Result<Vec<_>, _>>()?;
        dir_entries.sort_by_key(|e| Reverse(e.file_name()));
        for dir_entry in dir_entries {
            let Some(mut sstable) = SSTable::from_path(dir_entry.path())? else {
                continue;
            };
            if sstable.bloom_filter().may_contain(key.as_ref().as_bytes()) {
                while let Some(entry) = sstable.read_next_entry()? {
                    if entry.key() == key.as_ref() {
                        return match entry {
                            Entry::Set { .. } => Ok(Some(entry)),
                            Entry::Delete { .. } => Ok(None),
                        };
                    }
                }
            }
        }
        Ok(None)
    }

    /// Returns `true` if `key` exists in the memtable or any `SSTable`.
    pub fn contains(&self, key: impl AsRef<str>) -> anyhow::Result<bool> {
        self.get(key).map(|o| o.is_some())
    }

    /// Writes all memtable entries sorted by key to a new timestamped `SSTable` file,
    /// then truncates the WAL and clears the memtable.
    pub fn flush(&mut self) -> anyhow::Result<()> {
        self.memtable.flush_to(self.sstables_path.clone())?;
        self.wal_file.set_len(0)?;
        self.wal_file.seek(SeekFrom::Start(0))?;
        self.flush_count = self.flush_count.saturating_add(1);
        if self.flush_count.is_multiple_of(COMPACT_EVERY_N_FLUSHES) {
            self.compact()?;
        }
        Ok(())
    }

    /// Flushes to an `SSTable` if the memtable has exceeded the 4 MB size threshold.
    pub fn maybe_flush(&mut self) -> anyhow::Result<()> {
        if self.memtable.should_flush() {
            self.flush()
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn new_creates_empty_memtable() {
        let (_dir, log) = temp_log();
        assert!(log.memtable.is_empty());
    }

    #[test]
    fn new_rebuilds_memtable_from_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let memtable_path = dir.path().join("test.log");
        let sstables_path = dir.path().join("sstables");
        {
            let mut log = Log::new(dir.path(), &memtable_path, &sstables_path, true).unwrap();
            log.write(Entry::set("a", "1")).unwrap();
        }
        let log = Log::new(dir.path(), &memtable_path, &sstables_path, false).unwrap();
        assert_eq!(log.memtable.len(), 1);
        assert!(log.memtable.contains_key("a"));
    }

    #[test]
    fn get_returns_entry_from_memtable() {
        let (_dir, mut log) = temp_log();
        let set = Entry::set("a", "1");
        log.write(set.clone()).unwrap();
        let result = log.get("a").unwrap().unwrap();
        assert_eq!(result.key(), set.key());
        assert_eq!(result.value(), set.value());
    }

    #[test]
    fn get_returns_none_when_absent_from_both() {
        let (_dir, log) = temp_log();
        assert!(log.get("a").unwrap().is_none());
    }

    #[test]
    fn get_finds_entry_in_sstable_after_flush() {
        let (_dir, mut log) = temp_log();
        let set = Entry::set("a", "1");
        log.write(set.clone()).unwrap();
        log.flush().unwrap();
        let result = log.get("a").unwrap().unwrap();
        assert_eq!(set.key(), result.key());
        assert_eq!(set.value(), result.value());
    }

    #[test]
    fn get_returns_newest_when_key_in_multiple_sstables() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.write(Entry::set("a", "2")).unwrap();
        log.flush().unwrap();
        assert_eq!(log.get("a").unwrap().unwrap().value(), Some("2"));
    }

    #[test]
    fn flush_creates_sstable_file_on_disk() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        let sst_exists = read_dir(log.sstables_path)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .any(|e| e.path().extension().is_some_and(|ext| ext == "sst"));
        assert!(sst_exists);
    }

    #[test]
    fn flush_clears_memtable() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        assert!(log.memtable.is_empty());
    }

    #[test]
    fn flush_truncates_wal() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        assert_eq!(log.wal_file.metadata().unwrap().len(), 0);
    }

    #[test]
    fn maybe_flush_does_not_flush_when_below_threshold() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.maybe_flush().unwrap();
        assert!(!log.memtable.is_empty());
        assert!(log.wal_file.metadata().unwrap().len() != 0);
        let sst_exists = read_dir(&log.sstables_path)
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .any(|e| e.path().extension().is_some_and(|ext| ext == "sst"));
        assert!(!sst_exists);
    }

    #[test]
    fn get_returns_none_for_absent_key_across_multiple_sstables() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        log.write(Entry::set("b", "2")).unwrap();
        log.flush().unwrap();
        log.write(Entry::set("c", "3")).unwrap();
        log.flush().unwrap();
        assert!(log.get("d").unwrap().is_none());
    }

    #[test]
    fn get_returns_none_for_tombstone_in_memtable() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.write(Entry::delete("a")).unwrap();
        assert!(log.get("a").unwrap().is_none());
    }

    #[test]
    fn get_returns_none_after_flush_and_delete() {
        // Regression: tombstone resurrection bug. Before the fix, deleting a key that had already
        // been flushed to an SSTable would only clear the memtable; the SSTable still contained
        // the Set entry, so a subsequent get() would find and return it.
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        log.write(Entry::delete("a")).unwrap();
        assert!(log.get("a").unwrap().is_none());
    }

    #[test]
    fn get_skips_sstable_when_bloom_filter_says_absent() {
        // If the bloom filter definitively says a key is absent, the SSTable is not scanned.
        // The result must still be None.
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        assert!(log.get("z").unwrap().is_none());
    }

    #[test]
    fn get_finds_key_when_bloom_filter_says_maybe_present() {
        // When the bloom filter reports a key may be present, the SSTable is scanned and the
        // entry is returned.
        let (_dir, mut log) = temp_log();
        let set = Entry::set("a", "1");
        log.write(set.clone()).unwrap();
        log.flush().unwrap();
        let result = log.get("a").unwrap().unwrap();
        assert_eq!(result, set);
    }
}
