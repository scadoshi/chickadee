use super::serializer::HeaderSerializer;
use std::io::{Seek, SeekFrom, Write};
use wincode::{SchemaWrite, config::DefaultConfig};

/// On-disk record format: `[magic: 2B][crc32: 4B][entry_len: 4B][wincode payload]`.
pub(crate) trait HeaderWriter<T>
where
    T: SchemaWrite<DefaultConfig, Src = T>,
{
    /// Always appends, whatever the cursor was doing.
    fn header_write(&mut self, value: &T) -> anyhow::Result<()>;
}

impl<W, T> HeaderWriter<T> for W
where
    W: Write + Seek,
    T: SchemaWrite<DefaultConfig, Src = T>,
{
    fn header_write(&mut self, value: &T) -> anyhow::Result<()> {
        self.seek(SeekFrom::End(0))?;
        let bytes = HeaderSerializer::serialize(value)?;
        self.write_all(bytes.as_slice())?;
        Ok(())
    }
}
