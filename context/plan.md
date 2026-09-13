# chickadee

Log-structured key-value store in Rust. Named after the bird that caches thousands of seeds each autumn and grows extra hippocampus to remember where they went.

## Phase 1 — Append-only log with in-memory index

- Accept `get <key>` and `set <key> <value>` from stdin
- Append entries to a single log file on disk
- Maintain an in-memory HashMap of key → file offset for O(1) reads
- Rebuild index from log on startup
- Support `delete <key>` via tombstone entries

## Phase 2 — Durability and compaction

- Write-ahead log for crash recovery
- Log compaction — rewrite log keeping only latest value per key
- Handle partial writes and corruption gracefully

## Phase 3 — Binary serialization

- Replace plaintext log with binary format (serde + bincode)
- Fixed-size headers, length-prefixed values
- Checksums per entry for integrity

## Phase 4 — SSTable / LSM-tree

- In-memory memtable (sorted, e.g. BTreeMap)
- Flush memtable to sorted on-disk segments (SSTables)
- Merge/compact segments in background
- Bloom filters for fast negative lookups

## Phase 5 — Network layer

- TCP server with a simple wire protocol
- Client can connect and issue get/set/delete commands
- Request/response framing

## Phase 6 — Concurrency

- RwLock for concurrent readers, single writer
- Connection handling with tokio or std threads
- Explore MVCC if ambitious
