# chickadee

Log-structured key-value store in Rust.

Named after the bird that does this for a living. A chickadee caches thousands of seeds across its territory each autumn, and grows extra hippocampus to remember where it put them. Write it down, find it again later. That is the whole job.

The sibling project is `steller`, an in-memory Redis-compatible server. Another bird from the same forest, covering the other half of how a KV system gets built.

## Running

### CLI

Interactive terminal interface:

```
cargo run --bin cli
```

### TCP server

```
cp .env.template .env
cargo run --bin server
```

The server reads `ADDRESS` and `PORT` from `.env` (default template: `127.0.0.1:3000`).

Connect with netcat:

```
nc 127.0.0.1 3000
```

Multiple clients can connect concurrently — each connection gets its own thread, sharing the same underlying store.

### Tests

```
cargo test
```

99 tests covering unit, integration, and end-to-end TCP server behavior.

## Commands

```
set <key> <value>    (alias: s)
get <key>            (alias: g)
delete <key>         (alias: d, del)
quit                 (alias: q, exit)
help                 (alias: h)
```

Works identically in the CLI and over TCP.

## How it works

Chickadee is an LSM-tree (Log-Structured Merge-tree) key-value store.

**Write path**
- Writes append to a write-ahead log (WAL) and update an in-memory `BTreeMap` (the memtable)
- Each entry is wrapped in a 10-byte header: `[magic: 2B][crc32: 4B][entry_len: 4B]`
- Magic bytes (`0x4443`, "CD" in a hex dump) mark entry boundaries for corruption recovery
- CRC32 checksums detect corrupt entries; the reader scans byte-by-byte past bad data
- `sync_all()` is called after every write for durability
- When the memtable exceeds 4MB, it is flushed to a new timestamped SSTable file

**Read path**
- Reads check the memtable first, then scan SSTable files newest-to-oldest
- Each SSTable has a bloom filter (k=7, 10 bits/key, ~1% false positive rate) for fast negative lookups
- SSTable filenames are microsecond Unix timestamps, zero-padded — lexicographic order = chronological order

**Deletes**
- Delete writes a tombstone entry to both the WAL and memtable
- Tombstones propagate through SSTables to prevent resurrection of flushed keys
- Compaction drops tombstone winners so they don't accumulate

**Compaction**
- Every 10 flushes, all SSTables are merged via a k-way merge
- Files are processed in sorted key order simultaneously; the newest SSTable wins on duplicate keys
- Original SSTables are replaced by the compacted output

**Concurrency**
- The `Log` is wrapped in `Arc<Mutex<Log>>` and shared across connection threads
- Locking is per-command, not per-connection — threads only hold the lock for a single operation

## Project structure

```
src/
  lib/
    log/
      mod.rs          Log struct (write, get, contains, flush, compact)
      entry.rs        Entry enum (Set/Delete)
      command.rs      Command parsing, Execute trait
      memtable.rs     In-memory BTreeMap with flush-to-SSTable
      header/         Binary serialization (magic bytes, CRC32, corruption recovery)
      sstable/
        mod.rs        SSTable reader
        bloom_filter.rs   Bloom filter (xxh3, double hashing)
        compact.rs    K-way merge compaction
    run.rs            Runner<R, W> — generic over BufRead + Write
    tui.rs            Terminal chrome (welcome banner, help text)
  bin/
    cli.rs            CLI entry point
    server.rs         TCP server entry point
tests/
  server.rs           Integration tests (spawns server, connects via TcpStream)
```

Data is stored in `data/` in the working directory, created on first run.
