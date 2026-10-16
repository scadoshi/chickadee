# Todo

## Phases 1–6 — COMPLETE

A fully functional LSM-tree key-value store with a CLI, TCP server, and concurrent connection handling. 99 tests passing. See `context/progress.md` for full detail.

---

## Potential enhancements

### Storage
- [ ] Leveled compaction — L0 flush target, L1+ non-overlapping key ranges, 10x size ratio per level
- [ ] Sparse index / index block — avoid scanning every SSTable key; binary search between index points
- [ ] Bloom filter tuning — configurable bits-per-key and hash count via `.env`
- [ ] SSTable block compression (e.g. zstd) — reduce disk footprint

### Query
- [ ] `SCAN <start> <end>` command — range query over memtable + SSTables using sorted order
- [ ] `COMPACT` command — expose manual compaction trigger alongside auto-trigger

### Concurrency
- [ ] `RwLock<Log>` — allow concurrent readers, single writer (currently `Mutex` serializes all ops)
- [ ] MVCC — multi-version concurrency control for snapshot reads
- [ ] Connection limit — max open connections, queue or reject beyond threshold

### Protocol
- [ ] Pipelining — accept multiple commands before flushing responses
- [ ] Binary protocol option — length-prefixed framing, more efficient than newline text
- [ ] Client library — thin Rust crate wrapping `TcpStream` with typed request/response

### Observability
- [ ] Structured logging (`tracing` crate) — replace `eprintln!` with spans and events
- [ ] Metrics — key count, memtable size, SSTable count, compaction frequency
- [ ] `INFO` command — report current store stats to client

### Ops
- [ ] Graceful shutdown — drain in-flight connections on SIGTERM before exiting
- [ ] Configurable data path via `.env` — currently hardcoded to `data/`
- [ ] Health check endpoint — simple `PING` → `PONG` for load balancer probes

---

## Architecture notes

SSTable file layout:
```
[entry 0 with header][entry 1 with header]...[bloom_filter bytes][bit_count: 4B u32 LE]
```

Entry header format:
```
[magic: 2 bytes (0x4E48 "NH")][crc32: 4 bytes][entry_len: 4 bytes][wincode-serialized Entry]
```

Key files:
- `src/lib/log/entry.rs` — `Entry` enum (Set/Delete) — single type used by all layers
- `src/lib/log/header/` — `HeaderWriter`, `HeaderReader`, `HeaderSerializer`, `HeaderDeserializer`, `CorruptionType`
- `src/lib/log/mod.rs` — `Log` struct: `write`, `get`, `contains`, `flush`, `maybe_flush`
- `src/lib/log/memtable.rs` — `MemTable` wrapping `BTreeMap<String, Entry>`, `process()`, `flush_to()`, `should_flush()`
- `src/lib/log/sstable/mod.rs` — `SSTable` struct: bloom filter, boundary position, entry iteration
- `src/lib/log/sstable/bloom_filter.rs` — `BloomFilter`, `BloomFilterReader`
- `src/lib/log/sstable/compact.rs` — `Log::compact()` k-way merge; tombstone winners dropped
- `src/lib/log/command.rs` — `Execute` trait on `Log`, command dispatch, `writeln!` responses
- `src/lib/run.rs` — `Runner<R, W>` generic over `BufRead + Write`; per-command `Arc<Mutex<Log>>` locking
- `src/bin/cli.rs` — CLI entry point; `Runner` with stdin/stdout
- `src/bin/server.rs` — TCP server; `TcpListener` loop, thread-per-connection, shared `Arc<Mutex<Log>>`
- `tests/server.rs` — integration tests for TCP server (set/get/del/err/sequencing)

## Study list

- ~~`std::io::Seek`, `SeekFrom`, `stream_position()`~~ — learned in Phase 1
- ~~Bitcask paper~~ — read, using as model for Phase 2
- ~~`std::fs::metadata().len()`~~ — learned for size-based merge triggering
- ~~`std::fs::File::sync_all()`~~ — learned and implemented in Phase 2
- ~~CRC32 checksums (`crc32fast` crate)~~ — learned and implemented in Phase 3
- ~~`u32::to_le_bytes()` / `u32::from_le_bytes()`~~ — learned in Phase 3
- ~~`BTreeMap`~~ — sorted in-memory structure, understood as ordered map for memtable
- ~~`std::io::BufWriter`~~ — learned and used in merge for batched writes
- ~~SSTable format~~ — learned and implemented in Phase 4
- ~~LSM-tree architecture~~ — learned and implemented in Phase 4
- ~~Sorted merge (k-way merge)~~ — learned and implemented in Phase 4
- ~~Bloom filters~~ — learned and implemented: k=7 hashes, 10 bits/key, double-hashing, ~1% FP rate
- ~~Tombstone propagation in LSM-trees~~ — learned and implemented in Phase 4
- ~~TCP framing and wire protocols~~ — newline-delimited text; `BufReader::read_line()`, `writeln!` + `flush()`
- ~~`Arc<Mutex<T>>`~~ — learned and implemented in Phase 6; per-command locking for concurrent access
- ~~Thread-per-connection server pattern~~ — learned and implemented in Phase 6
- `RwLock` — concurrent readers, exclusive writer; next step after `Mutex`
- `tokio` async runtime — needed for high-concurrency beyond thread-per-connection
- MVCC — snapshot reads without blocking writers
