use super::deserializer::HeaderDeserializer;
use anyhow::Context;
use std::io::{Read, Seek, SeekFrom};
use wincode::{SchemaRead, config::DefaultConfig};

/// Reads the record format written by [`HeaderWriter`](super::writer::HeaderWriter).
pub(crate) trait HeaderReader<T>
where
    T: for<'de> SchemaRead<'de, DefaultConfig, Dst = T>,
{
    /// Next valid entry from the cursor. On corruption it walks forward a byte at a time
    /// until magic and checksum both line up again.
    fn header_read_next(&mut self) -> anyhow::Result<Option<T>>;
    /// Whether the file holds at least one valid entry. Does not move the cursor.
    fn header_has_at_least_one(&mut self) -> anyhow::Result<bool>;
}

impl<R, T> HeaderReader<T> for R
where
    R: Read + Seek,
    T: for<'de> SchemaRead<'de, DefaultConfig, Dst = T>,
{
    fn header_read_next(&mut self) -> anyhow::Result<Option<T>> {
        let pos = self.stream_position()?;
        let buf_len = {
            self.seek(SeekFrom::End(0))?;
            let buf_len = self.stream_position()?;
            self.seek(SeekFrom::Start(pos))?;
            buf_len
        };
        if pos >= buf_len {
            return Ok(None);
        }
        let mut bytes = Vec::<u8>::new();
        self.read_to_end(&mut bytes)?;
        for p in 0..bytes.len() {
            let Some(window) = bytes.get(p..) else { break };
            if let Ok((entry, len)) = HeaderDeserializer::deserialize(window) {
                let next = pos
                    .checked_add(u64::try_from(p)?)
                    .and_then(|n| n.checked_add(u64::try_from(len).ok()?))
                    .context("entry offset overflows u64")?;
                self.seek(SeekFrom::Start(next))?;
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    fn header_has_at_least_one(&mut self) -> anyhow::Result<bool> {
        let pos = self.stream_position()?;
        self.seek(SeekFrom::Start(0))?;
        let value: Option<T> = self.header_read_next()?;
        self.seek(SeekFrom::Start(pos))?;
        Ok(value.is_some())
    }
}
