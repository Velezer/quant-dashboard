use rusqlite::Connection;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JournalMode {
    Wal,
    Delete,
}

impl JournalMode {
    fn parse(value: Option<&str>) -> Result<Self, rusqlite::Error> {
        match value.map(str::trim).map(str::to_ascii_uppercase).as_deref() {
            Some("WAL") => Ok(Self::Wal),
            None | Some("DELETE") => Ok(Self::Delete),
            _ => Err(super::storage_error(
                "QUANT_SQLITE_JOURNAL_MODE must be WAL or DELETE",
            )),
        }
    }

    fn settings(self) -> (&'static str, &'static str) {
        match self {
            Self::Wal => ("WAL", "NORMAL"),
            Self::Delete => ("DELETE", "FULL"),
        }
    }
}

pub(in crate::database) fn configure(
    connection: &Connection,
    schemas: &[&str],
) -> Result<(), rusqlite::Error> {
    configure_with_mode(connection, schemas, configured_mode()?)
}

fn configured_mode() -> Result<JournalMode, rusqlite::Error> {
    let configured = std::env::var("QUANT_SQLITE_JOURNAL_MODE");
    let value = match &configured {
        Ok(value) => Some(value.as_str()),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(super::storage_error(
                "QUANT_SQLITE_JOURNAL_MODE must be WAL or DELETE",
            ));
        }
    };
    JournalMode::parse(value)
}

pub(in crate::database) fn prepare_database(
    path: &Path,
    create: bool,
) -> Result<(), rusqlite::Error> {
    prepare_database_with_mode(path, create, configured_mode()?)
}

fn prepare_database_with_mode(
    path: &Path,
    create: bool,
    mode: JournalMode,
) -> Result<(), rusqlite::Error> {
    if mode != JournalMode::Delete
        || path.as_os_str().is_empty()
        || path == Path::new(":memory:")
        || (!create && !path.is_file())
    {
        return Ok(());
    }
    static PREPARED_DATABASES: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    let mut prepared = PREPARED_DATABASES
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if prepared.contains(&key) {
        return Ok(());
    }
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
        | if create {
            rusqlite::OpenFlags::SQLITE_OPEN_CREATE
        } else {
            rusqlite::OpenFlags::empty()
        };
    let connection = Connection::open_with_flags(path, flags)?;
    // Set this before reading the schema so WAL recovery uses a private wal-index
    // instead of the volume's shared-memory file. Closing releases every lock;
    // normal runtime connections retain their default NORMAL locking mode.
    connection.execute_batch("PRAGMA busy_timeout=5000; PRAGMA locking_mode=EXCLUSIVE;")?;
    configure_with_mode(&connection, &["main"], mode)?;
    connection.close().map_err(|(_, error)| error)?;
    prepared.insert(path.canonicalize().map_err(super::io_sqlite_error)?);
    Ok(())
}

fn configure_with_mode(
    connection: &Connection,
    schemas: &[&str],
    mode: JournalMode,
) -> Result<(), rusqlite::Error> {
    let (journal, synchronous) = mode.settings();
    for schema in schemas {
        if !matches!(*schema, "main" | "app" | "signals") {
            return Err(super::storage_error("unsupported SQLite journal schema"));
        }
        let actual: String = connection.query_row(
            &format!("PRAGMA {schema}.journal_mode={journal}"),
            [],
            |row| row.get(0),
        )?;
        let is_in_memory = actual == "memory"
            && connection.query_row(
                "SELECT file='' FROM pragma_database_list WHERE name=?1",
                [schema],
                |row| row.get::<_, bool>(0),
            )?;
        if !actual.eq_ignore_ascii_case(journal) && !is_in_memory {
            return Err(super::storage_error(&format!(
                "SQLite {schema} could not use requested {journal} journal mode: {actual}"
            )));
        }
        connection.execute_batch(&format!("PRAGMA {schema}.synchronous={synchronous};"))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
