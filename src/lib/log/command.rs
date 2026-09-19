use super::{Log, entry::Entry};
use crate::tui;
use std::io::Write;
use thiserror::Error;

/// Why a line of input did not parse.
#[derive(Debug, Error)]
pub enum CommandError {
    /// The first word is not a command or alias.
    #[error("unrecognized command")]
    UnrecognizedCommand,
    /// Fewer arguments than the command takes.
    #[error("missing required arguments")]
    MissingRequiredArguments,
    /// More arguments than the command takes.
    #[error("too many arguments")]
    TooManyArguments,
}

/// A parsed line of user input. `Quit` and `Help` never touch the log.
#[derive(Debug)]
pub enum Command {
    /// Store a value under a key.
    Set { key: String, value: String },
    /// Look up a key.
    Get { key: String },
    /// Remove a key.
    Delete { key: String },
    /// Leave the REPL.
    Quit,
    /// Print the command list.
    Help,
}

impl TryFrom<&str> for Command {
    type Error = CommandError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut parts = value.split_whitespace();
        let Some(command_str) = parts.next().map(str::to_lowercase) else {
            return Err(Self::Error::MissingRequiredArguments);
        };

        match command_str.as_str() {
            "set" | "s" => {
                let (Some(key), Some(value)) = (
                    parts.next().map(std::string::ToString::to_string),
                    parts.next().map(std::string::ToString::to_string),
                ) else {
                    return Err(Self::Error::MissingRequiredArguments);
                };
                if parts.next().is_some() {
                    return Err(Self::Error::TooManyArguments);
                }
                Ok(Self::Set { key, value })
            }
            "get" | "g" => {
                let Some(key) = parts.next().map(std::string::ToString::to_string) else {
                    return Err(Self::Error::MissingRequiredArguments);
                };
                if parts.next().is_some() {
                    return Err(Self::Error::TooManyArguments);
                }
                Ok(Self::Get { key })
            }
            "delete" | "del" | "d" => {
                let Some(key) = parts.next().map(std::string::ToString::to_string) else {
                    return Err(Self::Error::MissingRequiredArguments);
                };
                if parts.next().is_some() {
                    return Err(Self::Error::TooManyArguments);
                }
                Ok(Self::Delete { key })
            }
            "quit" | "q" | "exit" => Ok(Self::Quit),
            "help" | "h" => Ok(Self::Help),
            _ => Err(Self::Error::UnrecognizedCommand),
        }
    }
}

impl Command {
    /// Builds a `Set`.
    pub fn set(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Set {
            key: key.into(),
            value: value.into(),
        }
    }

    /// Builds a `Get`.
    pub fn get(key: impl Into<String>) -> Self {
        Self::Get { key: key.into() }
    }

    /// Builds a `Delete`.
    pub fn delete(key: impl Into<String>) -> Self {
        Self::Delete { key: key.into() }
    }

    /// The key for `Set`, `Get`, and `Delete`; `None` otherwise.
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::Set { key, .. } | Self::Get { key } | Self::Delete { key } => Some(key.as_str()),
            Self::Quit | Self::Help => None,
        }
    }

    /// The value for `Set`; `None` otherwise.
    pub fn value(&self) -> Option<&str> {
        match self {
            Self::Set { value, .. } => Some(value.as_str()),
            Self::Get { .. } | Self::Delete { .. } | Self::Quit | Self::Help => None,
        }
    }
}

/// Runs commands against a log. A trait rather than inherent methods so command
/// handling sits next to `Command`.
pub trait Execute {
    /// Applies `command` and writes the response to `writer`.
    fn execute(&mut self, command: Command, writer: &mut impl Write) -> anyhow::Result<()>;
}

impl Execute for Log {
    fn execute(&mut self, command: Command, writer: &mut impl Write) -> anyhow::Result<()> {
        match command {
            Command::Set { key, value } => {
                writeln!(writer, "{key} => {value}")?;
                self.write(Entry::set(key, value))?;
                self.maybe_flush()?;
            }
            Command::Get { key } => match self.get(&key)? {
                Some(Entry::Set { value, .. }) => writeln!(writer, "{key} => {value}")?,
                Some(Entry::Delete { .. }) | None => writeln!(writer, "{key} not found")?,
            },
            Command::Delete { key } => {
                // No tombstone for a key we never had, so misses don't grow the log.
                if self.contains(&key)? {
                    writeln!(writer, "{key} deleted")?;
                    self.write(Entry::delete(key))?;
                } else {
                    writeln!(writer, "{key} not found")?;
                }
                self.maybe_flush()?;
            }
            // The run loop breaks on Quit, so nothing here ever has to exit the process.
            Command::Quit => anyhow::bail!("quit must be handled by the run loop"),
            Command::Help => writeln!(writer, "{}", tui::command_hint())?,
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn command_set_from_standard() {
        assert!(matches!(
            Command::try_from("set a b"),
            Ok(Command::Set { .. })
        ));
    }

    #[test]
    fn command_set_from_alias_s() {
        assert!(matches!(
            Command::try_from("s a b"),
            Ok(Command::Set { .. })
        ));
    }

    #[test]
    fn command_get_from_standard() {
        assert!(matches!(
            Command::try_from("get a"),
            Ok(Command::Get { .. })
        ));
    }

    #[test]
    fn command_get_from_alias_g() {
        assert!(matches!(Command::try_from("g a"), Ok(Command::Get { .. })));
    }

    #[test]
    fn command_delete_from_standard() {
        assert!(matches!(
            Command::try_from("delete a"),
            Ok(Command::Delete { .. })
        ));
    }

    #[test]
    fn command_delete_from_alias_del() {
        assert!(matches!(
            Command::try_from("del a"),
            Ok(Command::Delete { .. })
        ));
    }

    #[test]
    fn command_delete_from_alias_d() {
        assert!(matches!(
            Command::try_from("d a"),
            Ok(Command::Delete { .. })
        ));
    }

    #[test]
    fn command_quit_from_standard() {
        assert!(matches!(Command::try_from("quit"), Ok(Command::Quit)));
    }

    #[test]
    fn command_quit_from_alias_q() {
        assert!(matches!(Command::try_from("q"), Ok(Command::Quit)));
    }

    #[test]
    fn command_quit_from_alias_exit() {
        assert!(matches!(Command::try_from("exit"), Ok(Command::Quit)));
    }

    #[test]
    fn command_help_from_standard() {
        assert!(matches!(Command::try_from("help"), Ok(Command::Help)));
    }

    #[test]
    fn command_help_from_alias_h() {
        assert!(matches!(Command::try_from("h"), Ok(Command::Help)));
    }

    #[test]
    fn command_err_empty_input() {
        assert!(matches!(
            Command::try_from(""),
            Err(CommandError::MissingRequiredArguments)
        ));
    }

    #[test]
    fn command_err_set_missing_value() {
        assert!(matches!(
            Command::try_from("set a"),
            Err(CommandError::MissingRequiredArguments)
        ));
    }

    #[test]
    fn command_err_set_missing_key_and_value() {
        assert!(matches!(
            Command::try_from("set"),
            Err(CommandError::MissingRequiredArguments)
        ));
    }

    #[test]
    fn command_err_get_missing_key() {
        assert!(matches!(
            Command::try_from("get"),
            Err(CommandError::MissingRequiredArguments)
        ));
    }

    #[test]
    fn command_err_delete_missing_key() {
        assert!(matches!(
            Command::try_from("delete"),
            Err(CommandError::MissingRequiredArguments)
        ));
    }

    #[test]
    fn command_err_set_too_many_arguments() {
        assert!(matches!(
            Command::try_from("set a b c"),
            Err(CommandError::TooManyArguments)
        ));
    }

    #[test]
    fn command_err_get_too_many_arguments() {
        assert!(matches!(
            Command::try_from("get a b"),
            Err(CommandError::TooManyArguments)
        ));
    }

    #[test]
    fn command_err_delete_too_many_arguments() {
        assert!(matches!(
            Command::try_from("delete a b"),
            Err(CommandError::TooManyArguments)
        ));
    }

    #[test]
    fn command_err_unrecognized_command() {
        assert!(matches!(
            Command::try_from("foo"),
            Err(CommandError::UnrecognizedCommand)
        ));
    }

    fn temp_log() -> (tempfile::TempDir, Log) {
        let dir = tempdir().unwrap();
        let log = Log::new(
            dir.path(),
            dir.path().join("test.log"),
            dir.path().join("sstables"),
            true,
        )
        .unwrap();
        (dir, log)
    }

    fn run(log: &mut Log, cmd: Command) -> String {
        let mut out = Vec::<u8>::new();
        log.execute(cmd, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    // --- State tests ---

    #[test]
    fn execute_set_adds_to_memtable() {
        let (_dir, mut log) = temp_log();
        let cmd = Command::set("a", "1");
        let key = cmd.key().unwrap().to_string();
        log.execute(cmd, &mut std::io::sink()).unwrap();
        assert_eq!(log.memtable.len(), 1);
        assert!(log.memtable.contains_key(&key));
    }

    #[test]
    fn execute_delete_existing_key_tombstones_key() {
        let (_dir, mut log) = temp_log();
        log.execute(Command::set("a", "1"), &mut std::io::sink())
            .unwrap();
        log.execute(Command::delete("a"), &mut std::io::sink())
            .unwrap();
        assert!(log.get("a").unwrap().is_none());
    }

    #[test]
    fn execute_delete_missing_key_leaves_memtable_empty() {
        let (_dir, mut log) = temp_log();
        log.execute(Command::delete("a"), &mut std::io::sink())
            .unwrap();
        assert!(log.memtable.is_empty());
    }

    #[test]
    fn execute_delete_persists_tombstone() {
        let dir = tempfile::tempdir().unwrap();
        let memtable_path = dir.path().join("test.log");
        let sstables_path = dir.path().join("sstables");
        {
            let mut log = Log::new(dir.path(), &memtable_path, &sstables_path, true).unwrap();
            log.execute(Command::set("a", "1"), &mut std::io::sink())
                .unwrap();
            log.execute(Command::delete("a"), &mut std::io::sink())
                .unwrap();
        }
        let log = Log::new(dir.path(), &memtable_path, &sstables_path, false).unwrap();
        // tombstone is replayed from WAL into memtable
        assert!(!log.memtable.is_empty());
        assert!(log.get("a").unwrap().is_none());
    }

    #[test]
    fn execute_help_leaves_memtable_empty() {
        let (_dir, mut log) = temp_log();
        log.execute(Command::Help, &mut std::io::sink()).unwrap();
        assert!(log.memtable.is_empty());
    }

    #[test]
    fn execute_get_finds_key_in_sstable_after_flush() {
        let (_dir, mut log) = temp_log();
        log.execute(Command::set("a", "1"), &mut std::io::sink())
            .unwrap();
        log.flush().unwrap();
        assert!(log.contains("a").unwrap());
        log.execute(Command::get("a"), &mut std::io::sink())
            .unwrap();
    }

    // --- Output tests ---

    #[test]
    fn execute_set_writes_key_value() {
        let (_dir, mut log) = temp_log();
        assert_eq!(run(&mut log, Command::set("a", "1")).trim(), "a => 1");
    }

    #[test]
    fn execute_set_overwrite_writes_new_value() {
        let (_dir, mut log) = temp_log();
        run(&mut log, Command::set("a", "1"));
        assert_eq!(run(&mut log, Command::set("a", "2")).trim(), "a => 2");
    }

    #[test]
    fn execute_get_existing_writes_key_value() {
        let (_dir, mut log) = temp_log();
        run(&mut log, Command::set("a", "1"));
        assert_eq!(run(&mut log, Command::get("a")).trim(), "a => 1");
    }

    #[test]
    fn execute_get_missing_writes_not_found() {
        let (_dir, mut log) = temp_log();
        assert_eq!(run(&mut log, Command::get("a")).trim(), "a not found");
    }

    #[test]
    fn execute_delete_existing_writes_deleted() {
        let (_dir, mut log) = temp_log();
        run(&mut log, Command::set("a", "1"));
        assert_eq!(run(&mut log, Command::delete("a")).trim(), "a deleted");
    }

    #[test]
    fn execute_delete_missing_writes_not_found() {
        let (_dir, mut log) = temp_log();
        assert_eq!(run(&mut log, Command::delete("a")).trim(), "a not found");
    }

    #[test]
    fn execute_help_writes_command_list() {
        let (_dir, mut log) = temp_log();
        assert!(!run(&mut log, Command::Help).is_empty());
    }
}
