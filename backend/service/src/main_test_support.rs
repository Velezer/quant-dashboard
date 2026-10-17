use crate::now_millis;

pub(super) fn temporary_database_path(label: &str) -> String {
    std::env::temp_dir()
        .join(format!(
            "quant-{label}-{}-{}.sqlite3",
            std::process::id(),
            now_millis()
        ))
        .to_string_lossy()
        .into_owned()
}

pub(super) fn remove_temporary_database(path: &str) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}-wal"));
    let _ = std::fs::remove_file(format!("{path}-shm"));
}
