use super::SSTable;
use crate::log::{Log, entry::Entry, memtable::MemTable};
use std::{
    cmp::Reverse,
    collections::HashSet,
    fs::{read_dir, remove_file},
};

impl Log {
    /// compacts all `SSTables` into a compactd set using a k-way compact.
    /// Processes entries in sorted key order across all files simultaneously;
    /// when multiple `SSTables` contain the same key, the newest file wins.
    /// Intermediate output is flushed to new `SSTable` files when the 4 MB threshold
    /// is exceeded, with a final flush for any remaining entries.
    /// All original `SSTables` are deleted once the compactd output is written.
    pub fn compact(&mut self) -> anyhow::Result<()> {
        // Get entries and sort desc by path--will correlate to most recent ordering first
        let mut entries: Vec<_> = read_dir(&self.sstables_path)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|e| Reverse(e.file_name()));
        // Use this later for cleaning up old SSTables
        let to_delete: Vec<_> = entries.iter().map(std::fs::DirEntry::path).collect();
        // Build a (Entry, File) structure so we can keep track of latest entry and File to get
        // more entries
        let sstable_opts: Vec<Option<SSTable>> = entries
            .into_iter()
            .map(|e| SSTable::from_path(e.path()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut sstables: Vec<(Option<Entry>, SSTable)> = sstable_opts
            .into_iter()
            .flatten()
            .map(|sst| (None::<Entry>, sst))
            .collect();
        // Initialize first entry for every file
        for (entry, sstable) in &mut sstables {
            *entry = sstable.read_next_entry()?;
        }
        // For writing to
        let mut memtable = MemTable::new();
        // Track keys for which a winner (Set or Delete) has already been determined
        let mut seen_keys: HashSet<String> = HashSet::new();
        // Looping begins
        loop {
            // Retain non-exhausted files
            sstables.retain(|(entry, _)| entry.is_some());
            // Find min key; none means all files have been exhausted
            let Some(min) = sstables
                .iter()
                .filter_map(|(entry, _)| entry.as_ref())
                .map(|entry| entry.key().to_owned())
                .min()
            else {
                break;
            };
            // Write to memtable
            // First which includes min is winner; tombstone winners are dropped (not written)
            // All which include min should be advanced
            for (entry, sstable) in &mut sstables {
                let Some(entry_ref) = entry.as_ref() else {
                    continue;
                };
                let is_particpant = entry_ref.key() == min;
                let winner_found = seen_keys.contains(min.as_str());
                if is_particpant && !winner_found {
                    seen_keys.insert(min.clone());
                    if let Entry::Set { .. } = entry_ref {
                        memtable.process(entry_ref.clone())?;
                    }
                    // Entry::Delete: mark as seen but drop — tombstone served its purpose
                }
                if is_particpant {
                    *entry = sstable.read_next_entry()?;
                }
            }
            // Maintain in minimum memory store
            if memtable.should_flush() {
                memtable.flush_to(self.sstables_path.clone())?;
            }
        }
        // Final flush
        if !memtable.is_empty() {
            memtable.flush_to(self.sstables_path.clone())?;
        }
        // Delete old SSTables
        for path in to_delete {
            remove_file(path)?;
        }
        Ok(())
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
    fn compact_with_no_sstables_is_noop() {
        let (_dir, mut log) = temp_log();
        log.compact().unwrap();
        assert_eq!(
            read_dir(&log.sstables_path)
                .unwrap()
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "sst"))
                .count(),
            0
        );
    }

    #[test]
    fn compact_newest_wins_for_duplicate_key() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        let set2 = Entry::set("a", "2");
        log.write(set2.clone()).unwrap();
        log.flush().unwrap();
        log.compact().unwrap();
        assert_eq!(log.get("a").unwrap().unwrap().value(), set2.value());
    }

    #[test]
    fn compact_preserves_all_unique_keys() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        log.write(Entry::set("b", "2")).unwrap();
        log.flush().unwrap();
        log.compact().unwrap();
        log.get("a").unwrap().unwrap();
        log.get("b").unwrap().unwrap();
    }

    #[test]
    fn compact_deletes_original_sstables() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        log.write(Entry::set("b", "2")).unwrap();
        log.flush().unwrap();
        let existing_paths: Vec<_> = read_dir(&log.sstables_path)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        log.compact().unwrap();
        assert!(
            read_dir(&log.sstables_path)
                .unwrap()
                .flatten()
                .all(|e| !existing_paths.contains(&e.path()))
        );
    }

    #[test]
    fn compact_result_readable_via_get() {
        let (_dir, mut log) = temp_log();
        let set1 = Entry::set("a", "1");
        let set2 = Entry::set("b", "2");
        let set3 = Entry::set("c", "3");
        log.write(set1.clone()).unwrap();
        log.flush().unwrap();
        log.write(set2.clone()).unwrap();
        log.flush().unwrap();
        log.write(set3.clone()).unwrap();
        log.flush().unwrap();
        log.compact().unwrap();
        assert_eq!(log.get(set1.key()).unwrap().unwrap(), set1);
        assert_eq!(log.get(set2.key()).unwrap().unwrap(), set2);
        assert_eq!(log.get(set3.key()).unwrap().unwrap(), set3);
    }

    #[test]
    fn compact_reduces_sstable_count() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        log.write(Entry::set("a", "2")).unwrap();
        log.flush().unwrap();
        let count = read_dir(&log.sstables_path).unwrap().count();
        log.compact().unwrap();
        assert!(count > read_dir(&log.sstables_path).unwrap().count());
    }

    #[test]
    fn compact_single_sstable_produces_one_output_and_deletes_original() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        let original: Vec<_> = read_dir(&log.sstables_path)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        assert_eq!(original.len(), 1);
        log.compact().unwrap();
        assert!(!original[0].exists());
        assert_eq!(read_dir(&log.sstables_path).unwrap().flatten().count(), 1);
    }

    #[test]
    fn compact_three_sstables_with_overlapping_keys() {
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.write(Entry::set("b", "only")).unwrap();
        log.flush().unwrap();
        log.write(Entry::set("a", "2")).unwrap();
        log.write(Entry::set("c", "only")).unwrap();
        log.flush().unwrap();
        log.write(Entry::set("a", "3")).unwrap();
        log.flush().unwrap();
        log.compact().unwrap();
        assert_eq!(log.get("a").unwrap().unwrap().value(), Some("3"));
        assert!(log.get("b").unwrap().is_some());
        assert!(log.get("c").unwrap().is_some());
    }

    #[test]
    fn compact_drops_tombstone_from_output() {
        // Regression: tombstone resurrection via compact. A Delete entry that wins during
        // compaction must not appear in the compacted SSTable — it must be silently dropped so
        // that a subsequent get() returns None rather than resurrecting an older Set entry.
        let (_dir, mut log) = temp_log();
        log.write(Entry::set("a", "1")).unwrap();
        log.flush().unwrap();
        log.write(Entry::delete("a")).unwrap();
        log.flush().unwrap();
        log.compact().unwrap();
        assert!(log.get("a").unwrap().is_none());
    }
}
