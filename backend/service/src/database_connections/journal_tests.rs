use super::{configure_with_mode, prepare_database_with_mode, JournalMode};
use crate::main_test_support::{remove_temporary_database, temporary_database_path};
use rusqlite::Connection;

const SCHEMAS: [&str; 3] = ["main", "app", "signals"];

struct Databases([String; 3]);

impl Databases {
    fn new(label: &str) -> Self {
        Self(SCHEMAS.map(|schema| temporary_database_path(&format!("{label}-{schema}"))))
    }

    fn open(&self) -> Connection {
        let connection = Connection::open(&self.0[0]).unwrap();
        for (schema, path) in SCHEMAS[1..].iter().zip(&self.0[1..]) {
            connection
                .execute(&format!("ATTACH DATABASE ?1 AS {schema}"), [path])
                .unwrap();
        }
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        connection
    }
}

impl Drop for Databases {
    fn drop(&mut self) {
        for path in &self.0 {
            remove_temporary_database(path);
            let _ = std::fs::remove_file(format!("{path}-journal"));
        }
    }
}

fn assert_settings(connection: &Connection, journal: &str, synchronous: i64) {
    for schema in SCHEMAS {
        let actual: String = connection
            .query_row(&format!("PRAGMA {schema}.journal_mode"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(actual, journal);
        let actual: i64 = connection
            .query_row(&format!("PRAGMA {schema}.synchronous"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(actual, synchronous);
    }
}

#[test]
fn journal_configuration_accepts_only_durable_modes() {
    assert_eq!(JournalMode::parse(None).unwrap(), JournalMode::Wal);
    assert_eq!(JournalMode::parse(Some(" wal ")).unwrap(), JournalMode::Wal);
    assert_eq!(
        JournalMode::parse(Some("delete")).unwrap(),
        JournalMode::Delete
    );
    for invalid in ["", "OFF", "MEMORY", "DELETE; PRAGMA synchronous=OFF"] {
        assert!(JournalMode::parse(Some(invalid)).is_err());
    }
}

#[test]
fn rollback_journal_supports_attached_stores_and_multiple_connections() {
    let files = Databases::new("rollback-concurrent");
    let writer = files.open();
    configure_with_mode(&writer, &SCHEMAS, JournalMode::Delete).unwrap();
    assert_settings(&writer, "delete", 2);
    for schema in SCHEMAS {
        writer
            .execute_batch(&format!(
                "CREATE TABLE {schema}.saved(value INTEGER); INSERT INTO {schema}.saved VALUES(1);"
            ))
            .unwrap();
    }
    let reader = files.open();
    configure_with_mode(&reader, &SCHEMAS, JournalMode::Delete).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE;").unwrap();
    for schema in SCHEMAS {
        writer
            .execute_batch(&format!("UPDATE {schema}.saved SET value=2;"))
            .unwrap();
    }
    writer.execute_batch("COMMIT;").unwrap();
    for schema in SCHEMAS {
        let value: i64 = reader
            .query_row(&format!("SELECT value FROM {schema}.saved"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(value, 2);
    }
    drop(reader);
    drop(writer);
    for path in &files.0 {
        assert!(!std::path::Path::new(&format!("{path}-shm")).exists());
        assert!(!std::path::Path::new(&format!("{path}-wal")).exists());
    }
}

#[test]
fn switching_existing_wal_stores_preserves_committed_rows() {
    let files = Databases::new("wal-to-rollback");
    let original = files.open();
    configure_with_mode(&original, &SCHEMAS, JournalMode::Wal).unwrap();
    assert_settings(&original, "wal", 1);
    for schema in SCHEMAS {
        original.execute_batch(&format!(
            "CREATE TABLE {schema}.saved(value TEXT); INSERT INTO {schema}.saved VALUES('retained');"
        )).unwrap();
    }
    drop(original);
    let reopened = files.open();
    configure_with_mode(&reopened, &SCHEMAS, JournalMode::Delete).unwrap();
    assert_settings(&reopened, "delete", 2);
    for schema in SCHEMAS {
        let value: String = reopened
            .query_row(&format!("SELECT value FROM {schema}.saved"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(value, "retained");
    }
}

#[test]
fn in_memory_stores_keep_memory_journals() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH ':memory:' AS app; ATTACH ':memory:' AS signals;")
        .unwrap();
    configure_with_mode(&connection, &SCHEMAS, JournalMode::Delete).unwrap();
    assert_settings(&connection, "memory", 2);
}

#[test]
fn preparation_preserves_sqlite_ephemeral_path_semantics() {
    for path in ["", ":memory:"] {
        prepare_database_with_mode(std::path::Path::new(path), true, JournalMode::Delete).unwrap();
    }
}

#[test]
fn file_backed_stores_cannot_silently_keep_memory_journals() {
    let files = Databases::new("reject-persistent-memory-journal");
    let connection = files.open();
    connection
        .execute_batch(
            "PRAGMA main.journal_mode=MEMORY;
         CREATE TABLE main.saved(value INTEGER);
         BEGIN; INSERT INTO main.saved VALUES(1);",
        )
        .unwrap();
    let error = configure_with_mode(&connection, &["main"], JournalMode::Delete).unwrap_err();
    assert!(error.to_string().contains("could not use requested DELETE"));
    connection.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn unclean_wal_fixture() {
    let Ok(main) = std::env::var("QUANT_JOURNAL_TEST_MAIN") else {
        return;
    };
    let files = Databases([
        main,
        std::env::var("QUANT_JOURNAL_TEST_APP").unwrap(),
        std::env::var("QUANT_JOURNAL_TEST_SIGNALS").unwrap(),
    ]);
    let connection = files.open();
    configure_with_mode(&connection, &SCHEMAS, JournalMode::Wal).unwrap();
    connection
        .execute_batch("PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    for schema in SCHEMAS {
        connection.execute_batch(&format!(
            "CREATE TABLE {schema}.saved(value TEXT); INSERT INTO {schema}.saved VALUES('wal-only');"
        )).unwrap();
    }
    // Exit without running connection destructors, leaving committed WAL frames.
    std::process::exit(0);
}

#[test]
fn preparation_recovers_unclean_wal_and_releases_exclusive_locks() {
    let files = Databases::new("unclean-wal-preparation");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "database::connections::journal::tests::unclean_wal_fixture",
        ])
        .env("QUANT_JOURNAL_TEST_MAIN", &files.0[0])
        .env("QUANT_JOURNAL_TEST_APP", &files.0[1])
        .env("QUANT_JOURNAL_TEST_SIGNALS", &files.0[2])
        .status()
        .unwrap();
    assert!(status.success());
    for path in &files.0 {
        assert!(std::fs::metadata(format!("{path}-wal")).unwrap().len() > 0);
        prepare_database_with_mode(std::path::Path::new(path), false, JournalMode::Delete).unwrap();
    }
    let connection = files.open();
    assert_settings(&connection, "delete", 2);
    for schema in SCHEMAS {
        let value: String = connection
            .query_row(&format!("SELECT value FROM {schema}.saved"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(value, "wal-only");
        let locking: String = connection
            .query_row(&format!("PRAGMA {schema}.locking_mode"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(locking, "normal");
    }
    connection.execute_batch("BEGIN IMMEDIATE;").unwrap();
    prepare_database_with_mode(
        std::path::Path::new(&files.0[0]),
        false,
        JournalMode::Delete,
    )
    .unwrap();
    connection.execute_batch("ROLLBACK;").unwrap();
}
