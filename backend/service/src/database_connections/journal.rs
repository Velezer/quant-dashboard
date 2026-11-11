use rusqlite::Connection;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const SQLITE_IOERR_SHMSIZE: i32 = 4874;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JournalMode {
    Wal,
    Delete,
}

impl JournalMode {
    fn parse(value: Option<&str>) -> Result<Self, rusqlite::Error> {
        match value.map(str::trim).map(str::to_ascii_uppercase).as_deref() {
            Some("WAL") => Ok(Self::Wal),
            None => Ok(Self::Wal),
            Some("DELETE") => Ok(Self::Delete),
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
    let (mode, is_default) = configured_mode()?;
    configure_with_policy(connection, schemas, mode, is_default)
}

fn configure_with_policy(
    connection: &Connection,
    schemas: &[&str],
    mode: JournalMode,
    is_default: bool,
) -> Result<(), rusqlite::Error> {
    if mode != JournalMode::Wal || !is_default {
        return configure_with_mode(connection, schemas, mode);
    }
    for schema in schemas {
        if !matches!(*schema, "main" | "app" | "signals") {
            return Err(super::storage_error("unsupported SQLite journal schema"));
        }
        let path: String = connection.query_row(
            "SELECT file FROM pragma_database_list WHERE name=?1",
            [schema],
            |row| row.get(0),
        )?;
        let effective_mode = prepared_databases()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&database_key(Path::new(&path)))
            .copied()
            .unwrap_or(mode);
        configure_with_mode(connection, &[*schema], effective_mode)?;
    }
    Ok(())
}

fn configured_mode() -> Result<(JournalMode, bool), rusqlite::Error> {
    let configured = std::env::var("QUANT_SQLITE_JOURNAL_MODE");
    let (value, is_default) = match &configured {
        Ok(value) => (Some(value.as_str()), false),
        Err(std::env::VarError::NotPresent) => (None, true),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(super::storage_error(
                "QUANT_SQLITE_JOURNAL_MODE must be WAL or DELETE",
            ));
        }
    };
    Ok((JournalMode::parse(value)?, is_default))
}

pub(in crate::database) fn prepare_database(
    path: &Path,
    create: bool,
) -> Result<(), rusqlite::Error> {
    let (mode, is_default) = configured_mode()?;
    if mode == JournalMode::Wal {
        prepare_wal_database(path, create, is_default, probe_wal_database).map(|_| ())
    } else {
        prepare_database_with_mode(path, create, mode)
    }
}

fn prepared_databases() -> &'static Mutex<HashMap<PathBuf, JournalMode>> {
    static PREPARED_DATABASES: OnceLock<Mutex<HashMap<PathBuf, JournalMode>>> = OnceLock::new();
    PREPARED_DATABASES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn database_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn is_persistent_database(path: &Path, create: bool) -> bool {
    !path.as_os_str().is_empty()
        && path != Path::new(":memory:")
        && (create || path.is_file())
}

fn prepare_wal_database(
    path: &Path,
    create: bool,
    allow_fallback: bool,
    probe: impl FnOnce(&Path, bool) -> Result<(), rusqlite::Error>,
) -> Result<JournalMode, rusqlite::Error> {
    if !is_persistent_database(path, create) {
        return Ok(JournalMode::Wal);
    }
    let mut prepared = prepared_databases()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(mode) = prepared.get(&database_key(path)).copied() {
        if mode == JournalMode::Wal || allow_fallback {
            return Ok(mode);
        }
    }
    let mode = match probe(path, create) {
        Ok(()) => JournalMode::Wal,
        Err(error) if allow_fallback && is_wal_shared_memory_error(&error) => {
            // The probe has closed its connection. Recover through a fresh
            // exclusive connection before ATTACH can touch a shared wal-index.
            recover_delete_database(path, create)?;
            eprintln!(
                "[SQLite] WAL shared-memory initialization failed for {}; using DELETE journaling for this database after recovery.",
                path.display()
            );
            JournalMode::Delete
        }
        Err(error) => return Err(error),
    };
    prepared.insert(database_key(path), mode);
    Ok(mode)
}

fn probe_wal_database(path: &Path, create: bool) -> Result<(), rusqlite::Error> {
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
        | if create {
            rusqlite::OpenFlags::SQLITE_OPEN_CREATE
        } else {
            rusqlite::OpenFlags::empty()
        };
    let connection = Connection::open_with_flags(path, flags)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    let result = configure_with_mode(&connection, &["main"], JournalMode::Wal).and_then(|()| {
        // A read of an empty WAL database need not initialize shared memory.
        // Take and roll back a write transaction without changing any rows.
        connection.execute_batch(
            "BEGIN IMMEDIATE; SELECT count(*) FROM main.sqlite_schema; ROLLBACK;",
        )
    });
    match result {
        Ok(()) => connection.close().map_err(|(_, error)| error),
        Err(error) => {
            drop(connection);
            Err(error)
        }
    }
}

fn prepare_database_with_mode(
    path: &Path,
    create: bool,
    mode: JournalMode,
) -> Result<(), rusqlite::Error> {
    if mode != JournalMode::Delete || !is_persistent_database(path, create) {
        return Ok(());
    }
    let mut prepared = prepared_databases()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if prepared.get(&database_key(path)) == Some(&JournalMode::Delete) {
        return Ok(());
    }
    recover_delete_database(path, create)?;
    prepared.insert(database_key(path), JournalMode::Delete);
    Ok(())
}

fn recover_delete_database(path: &Path, create: bool) -> Result<(), rusqlite::Error> {
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
    configure_with_mode(&connection, &["main"], JournalMode::Delete)?;
    connection.close().map_err(|(_, error)| error)?;
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
        let mut actual: String = connection.query_row(
            &format!("PRAGMA {schema}.journal_mode"),
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
            actual = connection.query_row(
                &format!("PRAGMA {schema}.journal_mode={journal}"),
                [],
                |row| row.get(0),
            )?;
        }
        if !actual.eq_ignore_ascii_case(journal) && !is_in_memory {
            return Err(super::storage_error(&format!(
                "SQLite {schema} could not use requested {journal} journal mode: {actual}"
            )));
        }
        connection.execute_batch(&format!("PRAGMA {schema}.synchronous={synchronous};"))?;
    }
    Ok(())
}

fn is_wal_shared_memory_error(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(code, _) if code.extended_code == SQLITE_IOERR_SHMSIZE
    )
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
