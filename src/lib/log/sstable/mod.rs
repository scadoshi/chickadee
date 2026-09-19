pub(super) mod bloom_filter;
pub(super) mod compact;

use self::bloom_filter::{BloomFilter, BloomFilterReader};
use super::{entry::Entry, header::reader::HeaderReader};
use anyhow::Context;
use std::{fs::File, io::Seek, path::Path};

/// One flushed memtable: header-prefixed [`Entry`] records in key order, then a
/// [`BloomFilter`] footer so lookups can skip files that cannot hold the key.
#[derive(Debug)]
pub(super) struct SSTable {
    bloom_filter: BloomFilter,
    bloom_filter_pos: u64,
    file: File,
}

impl SSTable {
    /// `None` if the file is too small for a footer or holds no valid entry.
    pub(super) fn from_path(path: impl AsRef<Path>) -> anyhow::Result<Option<Self>> {
        let mut file = File::open(path.as_ref())?;
        let Some(bloom_filter) = file.read_bloom_filter()? else {
            return Ok(None);
        };
        let footer_len = u64::try_from(bloom_filter.len())?
            .checked_add(4)
            .context("bloom filter footer too large")?;
        let bloom_filter_pos = file
            .metadata()?
            .len()
            .checked_sub(footer_len)
            .context("sstable shorter than its bloom filter footer")?;
        if !HeaderReader::<Entry>::header_has_at_least_one(&mut file)? {
            return Ok(None);
        }
        Ok(Some(Self {
            bloom_filter,
            bloom_filter_pos,
            file,
        }))
    }

    pub(super) fn bloom_filter(&self) -> &BloomFilter {
        &self.bloom_filter
    }

    /// `None` once the cursor reaches the bloom filter footer.
    pub(super) fn read_next_entry(&mut self) -> anyhow::Result<Option<Entry>> {
        if self.file.stream_position()? > self.bloom_filter_pos {
            return Ok(None);
        }
        self.file.header_read_next()
    }
}
