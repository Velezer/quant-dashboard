use super::*;

fn register_request() -> RegisterRequest {
    RegisterRequest {
        name: "Owned SMA".to_string(),
        source: "//@version=6\nindicator(\"Owned SMA\", overlay=true)\nplot(ta.sma(close, 3))"
            .to_string(),
        author: "Me".to_string(),
        source_reference: "original work".to_string(),
        rights_basis: "original".to_string(),
        license_or_permission: "".to_string(),
        attribution: "None required by the applicable terms".to_string(),
        rights_confirmed: true,
    }
}

fn app_connection() -> Connection {
    let connection = Connection::open_in_memory().expect("in-memory database");
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app;")
        .expect("attach app database");
    initialize_schema(&connection).expect("Pine registry schema");
    connection
}

#[test]
fn registration_requires_rights_attestation_and_provenance() {
    let connection = app_connection();
    let mut request = register_request();
    request.rights_confirmed = false;
    assert!(
        matches!(register(&connection, request), Err(RegistryError::Invalid(message)) if message.contains("Confirm"))
    );

    let mut request = register_request();
    request.rights_basis = "license".to_string();
    assert!(
        matches!(register(&connection, request), Err(RegistryError::Invalid(message)) if message.contains("license terms"))
    );
}

#[test]
fn registration_preserves_source_and_rights_metadata_in_sqlite() {
    let connection = app_connection();
    let request = register_request();
    let source = request.source.clone();
    let record = register(&connection, request).expect("register user-owned Pine source");
    assert_eq!(record.source, source);
    assert_eq!(record.rights_basis, "original");
    assert_eq!(record.attribution, "None required by the applicable terms");
    let loaded = list(&connection, Some(record.id))
        .expect("read registered source")
        .remove(0);
    assert_eq!(loaded.source, source);
    assert_eq!(loaded.author, "Me");
}

#[test]
fn registration_rejects_unsupported_programs() {
    let connection = app_connection();
    let mut request = register_request();
    request.source = "//@version=6\nstrategy(\"Orders\")".to_string();
    assert!(
        matches!(register(&connection, request), Err(RegistryError::Invalid(message)) if message.contains("strategy()"))
    );
}

#[test]
fn strategy_registration_uses_its_own_provenance_registry() {
    let connection = app_connection();
    let mut request = register_request();
    request.name = "Owned strategy".to_string();
    request.source = "//@version=6\nstrategy(\"Owned strategy\")\nif bar_index == 0\n    strategy.entry(\"Long\", strategy.long)".to_string();
    let source = request.source.clone();
    let record = register_strategy(&connection, request).expect("register supported Pine strategy");
    assert_eq!(record.source, source);
    assert_eq!(record.rights_basis, "original");
    assert!(list(&connection, Some(record.id))
        .expect("indicator registry query")
        .is_empty());
    let loaded = list_strategies(&connection, Some(record.id))
        .expect("strategy registry query")
        .remove(0);
    assert_eq!(loaded.source, source);
    assert_eq!(loaded.name, "Owned strategy");
    assert_eq!(
        list_strategy_summaries(&connection)
            .expect("strategy summary list")
            .len(),
        1
    );
}
