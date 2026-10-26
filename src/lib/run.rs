use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

use super::log::Log;
use super::log::command::{Command, Execute};

/// Reads commands from `reader` a line at a time and writes each response to `writer`.
/// The CLI runs one over stdin and stdout; the server runs one per connection.
pub struct Runner<R, W>
where
    R: BufRead,
    W: Write,
{
    reader: R,
    writer: W,
}

impl<R, W> std::fmt::Debug for Runner<R, W>
where
    R: BufRead,
    W: Write,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner").finish_non_exhaustive()
    }
}

impl<R, W> Runner<R, W>
where
    R: BufRead,
    W: Write,
{
    /// A runner over `reader` and `writer`.
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "callers hand over their own Arc clone; keeping the signature avoids an API change"
    )]
    /// Runs every command `reader` sends against `log` until the input ends or a
    /// `quit` arrives. A line that does not parse gets an `Error:` reply and the
    /// loop goes on.
    ///
    /// # Errors
    ///
    /// Any I/O error from reading input or writing a response, an error from
    /// running a command, or a poisoned `log` mutex.
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
