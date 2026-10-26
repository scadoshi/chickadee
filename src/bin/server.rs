//! The TCP server: listens on `ADDRESS:PORT` from `.env` and runs one command loop
//! per connection against a shared store in `data/`.

use std::io::{BufReader, BufWriter};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use chickadee::log::{DATA_PATH, Log, SSTABLES_PATH, WAL_PATH};
use chickadee::run::Runner;

#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a server's own startup and connection lines go to its terminal"
)]
fn server() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let address = std::env::var("ADDRESS")?;
    let port = std::env::var("PORT")?;
    let bind_address = format!("{address}:{port}");
    let log = Arc::new(Mutex::new(Log::new(
        DATA_PATH,
        WAL_PATH,
        SSTABLES_PATH,
        /* truncate */ false,
    )?));
    let listener = TcpListener::bind(bind_address)?;
    println!("Listening on {}", listener.local_addr()?);
    for stream in listener.incoming() {
        let stream = stream?;
        let log = Arc::clone(&log);
        std::thread::spawn(move || {
            let Ok(stream_clone) = stream.try_clone() else {
                eprintln!("Failed to clone stream");
                return;
            };
            let reader = BufReader::new(stream_clone);
            let writer = BufWriter::new(stream);
            let mut runner = Runner::new(reader, writer);
            if let Err(e) = runner.run(log) {
                eprintln!("Connection error: {e}");
            }
        });
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    server()
}
