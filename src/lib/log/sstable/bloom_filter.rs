use anyhow::Context;
use std::{
    io::{Read, Seek, SeekFrom},
    ops::{Deref, DerefMut},
};

/// Bit array with `k = 7` positions per key. Any unset position means the key is absent;
/// all set means it may be present.
///
/// Stored as a footer at the end of each `SSTable` file: `[filter_bytes][bit_count: u32 le]`.
#[derive(Debug)]
pub(crate) struct BloomFilter {
    bit_count: usize,
    inner: Vec<u8>,
}

impl Deref for BloomFilter {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for BloomFilter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Enhanced double hashing: `pos_i = (h1 + i * h2) % bit_count`, with `h1` and `h2` being
/// xxh3-64 of the key under seeds 0 and 1.
fn positions(key: &[u8], bit_count: usize) -> impl Iterator<Item = usize> {
    let h1 = xxh3::hash64_with_seed(key, 0);
    let h2 = xxh3::hash64_with_seed(key, 1);
    let bit_count = u64::try_from(bit_count).unwrap_or(u64::MAX);
    (0..7u64).filter_map(move |i| {
        let pos = h1.wrapping_add(i.wrapping_mul(h2)).checked_rem(bit_count)?;
        usize::try_from(pos).ok()
    })
}

impl BloomFilter {
    /// At 10 bits per key, `k = 7` gives about 0.8% false positives.
    pub(crate) fn new(bit_count: usize) -> Self {
        let byte_count = bit_count.div_ceil(8);
        Self {
            bit_count,
            inner: vec![0; byte_count],
        }
    }

    /// Sets all 7 positions for `key`.
    pub(crate) fn insert(&mut self, key: &[u8]) {
        for pos in positions(key, self.bit_count) {
            if let Some(byte) = self.get_mut(pos / 8) {
                *byte |= 1 << (pos % 8);
            }
        }
    }

    /// `true` can be a false positive. `false` is always right.
    pub(crate) fn may_contain(&self, key: &[u8]) -> bool {
        positions(key, self.bit_count).all(|pos| {
            self.get(pos / 8)
                .is_none_or(|byte| byte & 1 << (pos % 8) != 0)
        })
    }
}

/// Read from the end of the file backward: the last 4 bytes are `bit_count` as a
/// little-endian `u32`, and the `bit_count.div_ceil(8)` bytes before that are the filter.
pub(super) trait BloomFilterReader: Read + Seek {
    /// `None` if the file is too small for a footer. Leaves the cursor at the start of the file.
    fn read_bloom_filter(&mut self) -> anyhow::Result<Option<BloomFilter>>;
}

impl<R: Read + Seek> BloomFilterReader for R {
    fn read_bloom_filter(&mut self) -> anyhow::Result<Option<BloomFilter>> {
        self.seek(SeekFrom::End(0))?;
        let size = self.stream_position()?;
        if size < 4 {
            return Ok(None);
        }
        self.seek(SeekFrom::End(-4))?;
        let mut bit_count_bytes = [0u8; 4];
        self.read_exact(&mut bit_count_bytes)?;
        let bit_count = usize::try_from(u32::from_le_bytes(bit_count_bytes))?;
        let byte_count = bit_count.div_ceil(8);
        let footer_len = u64::try_from(byte_count)?
            .checked_add(4)
            .context("bloom filter footer too large")?;
        let Some(filter_start) = size.checked_sub(footer_len) else {
            return Ok(None);
        };
        self.seek(SeekFrom::Start(filter_start))?;
        let mut inner = vec![0u8; byte_count];
        self.read_exact(&mut inner)?;
        self.seek(SeekFrom::Start(0))?;
        Ok(Some(BloomFilter { bit_count, inner }))
    }
}
