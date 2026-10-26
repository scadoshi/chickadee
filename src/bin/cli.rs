//! The interactive CLI: a read-eval-print loop over stdin and stdout against the
//! store in `data/`.

use std::io::{self, BufReader};
use std::sync::{Arc, Mutex};

use chickadee::log::{DATA_PATH, Log, SSTABLES_PATH, WAL_PATH};
use chickadee::run::Runner;
use chickadee::tui;

fn repl() -> anyhow::Result<()> {
    tui::welcome();
    let log = Arc::new(Mutex::new(Log::new(
        DATA_PATH,
        WAL_PATH,
        SSTABLES_PATH,
        /* truncate */ false,
    )?));
    let mut runner = Runner::new(BufReader::new(io::stdin().lock()), io::stdout().lock());
    runner.run(log)
}

fn main() -> anyhow::Result<()> {
    repl()
}
