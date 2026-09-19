use wincode::{SchemaRead, SchemaWrite};

/// What gets written to the WAL and `SSTable`s.
#[derive(Debug, SchemaRead, SchemaWrite, Clone, PartialEq)]
pub enum Entry {
    Set {
        key: String,
        value: String,
    },
    /// Tombstone.
    Delete {
        key: String,
    },
}

impl Entry {
    pub fn key(&self) -> &str {
        match self {
            Self::Set { key, .. } | Self::Delete { key } => key.as_str(),
        }
    }

    pub fn value(&self) -> Option<&str> {
        match self {
            Self::Set { value, .. } => Some(value.as_str()),
            Self::Delete { .. } => None,
        }
    }

    pub fn set(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Set {
            key: key.into(),
            value: value.into(),
        }
    }

    pub fn delete(key: impl Into<String>) -> Self {
        Self::Delete { key: key.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_set_key_returns_key() {
        let key = "a".to_string();
        let set = Entry::set(key.clone(), "1");
        assert_eq!(set.key(), key.as_str());
    }

    #[test]
    fn entry_delete_key_returns_key() {
        let key = "a".to_string();
        let delete = Entry::delete(key.clone());
        assert_eq!(delete.key(), key);
    }

    #[test]
    fn entry_set_value_returns_value() {
        let value = "1".to_string();
        let set = Entry::set("a", value.clone());
        assert_eq!(set.value(), Some(value.as_str()));
    }

    #[test]
    fn entry_delete_value_returns_none() {
        let delete = Entry::delete("a");
        assert_eq!(delete.value(), None);
    }
}
