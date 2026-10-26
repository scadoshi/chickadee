//! chickadee: a log-structured key-value store. Writes go to a write-ahead log and an
//! in-memory table, which flushes to sorted `SSTable` files that compaction merges.

/// The store: write-ahead log, memtable, `SSTable`s, and the commands that run against them.
pub mod log;
/// Reads commands a line at a time and runs them against a shared log, for the CLI and the server.
pub mod run;
/// The CLI's banner and command list.
pub mod tui;
