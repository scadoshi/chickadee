use super::log::{
    Log,
    command::{Command, Execute},
};
use std::{
    io::{BufRead, Write},
    sync::{Arc, Mutex},
};

pub struct Runner<R, W>
where
    R: BufRead,
    W: Write,
{
    reader: R,
    writer: W,
}

impl<R, W> Runner<R, W>
where
    R: BufRead,
    W: Write,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "callers hand over their own Arc clone; keeping the signature avoids an API change"
    )]
    pub fn run(&mut self, log: Arc<Mutex<Log>>) -> anyhow::Result<()> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.reader.read_line(&mut line)? == 0 {
                break;
            }
            match Command::try_from(line.trim()) {
                Ok(Command::Quit) => break,
                Ok(cmd) => {
                    let mut guard = log
                        .lock()
                        .map_err(|_| anyhow::anyhow!("log mutex poisoned"))?;
                    guard.execute(cmd, &mut self.writer)?;
                }
                Err(e) => writeln!(self.writer, "Error: {e}")?,
            }
            self.writer.flush()?;
        }
        Ok(())
    }
}
