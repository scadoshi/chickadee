#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use chickadee::{log::Log, run::Runner};
use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};
use tempfile::tempdir;

/// Spawns a server on a random port and returns its address.
fn start_server() -> std::net::SocketAddr {
    let dir = tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let wal_path = dir.path().join("wal");
    let sstables_path = dir.path().join("sstables");
    let log = Arc::new(Mutex::new(
        Log::new(dir.path(), wal_path, sstables_path, true).unwrap(),
    ));

    thread::spawn(move || {
        // Moved in so the tempdir outlives the server thread.
        let _dir = dir;
        for stream in listener.incoming() {
            let log = Arc::clone(&log);
            let stream = stream.unwrap();
            let reader = BufReader::new(stream.try_clone().unwrap());
            let writer = BufWriter::new(stream);
            let mut runner = Runner::new(reader, writer);
            runner.run(log).unwrap();
        }
    });

    addr
}

fn send(stream: &mut BufReader<TcpStream>, writer: &mut impl Write, cmd: &str) -> String {
    writeln!(writer, "{cmd}").unwrap();
    writer.flush().unwrap();
    let mut response = String::new();
    stream.read_line(&mut response).unwrap();
    response.trim().to_string()
}

// --- set ---

#[test]
fn set_returns_ok() {
    let addr = start_server();
    let stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = BufWriter::new(stream);
    let response = send(&mut reader, &mut writer, "set a 1");
    assert_eq!(response, "a => 1");
}

// --- get ---

#[test]
fn get_existing_key_returns_value() {
    let addr = start_server();
    let stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = BufWriter::new(stream);
    send(&mut reader, &mut writer, "set a 1");
    let response = send(&mut reader, &mut writer, "get a");
    assert_eq!(response, "a => 1");
}

#[test]
fn get_missing_key_returns_not_found() {
    let addr = start_server();
    let stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = BufWriter::new(stream);
    let response = send(&mut reader, &mut writer, "get missing");
    assert_eq!(response, "missing not found");
}

// --- del ---

#[test]
fn del_existing_key_returns_ok() {
    let addr = start_server();
    let stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = BufWriter::new(stream);
    send(&mut reader, &mut writer, "set a 1");
    let response = send(&mut reader, &mut writer, "del a");
    assert_eq!(response, "a deleted");
}

#[test]
fn del_missing_key_returns_not_found() {
    let addr = start_server();
    let stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = BufWriter::new(stream);
    let response = send(&mut reader, &mut writer, "del missing");
    assert_eq!(response, "missing not found");
}

// --- Error handling ---

#[test]
fn invalid_command_returns_err() {
    let addr = start_server();
    let stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = BufWriter::new(stream);
    let response = send(&mut reader, &mut writer, "invalid");
    assert_eq!(response, "Error: unrecognized command");
}

// --- Sequencing ---

#[test]
fn set_then_del_then_get_returns_not_found() {
    let addr = start_server();
    let stream = TcpStream::connect(addr).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = BufWriter::new(stream);
    send(&mut reader, &mut writer, "set a 1");
    send(&mut reader, &mut writer, "del a");
    let response = send(&mut reader, &mut writer, "get a");
    assert_eq!(response, "a not found");
}
