use super::*;

#[test]
fn generated_model_ddl_is_single_quoted_statement_with_pk_semantics() {
    let ddl = generate_ddl(&model()).unwrap();
    assert!(ddl.starts_with("CREATE TABLE \"analytics\".\"orders\""));
    assert!(ddl.contains("\"OrderId\" BIGINT NOT NULL"));
    assert!(ddl.contains("PRIMARY KEY (\"OrderId\")"));
    let statements = Parser::parse_sql(&DuckDbDialect {}, &ddl).unwrap();
    assert_eq!(statements.len(), 1);
    assert!(matches!(
        statements.first(),
        Some(Statement::CreateTable(_))
    ));
}

#[test]
fn arbitrary_saved_ddl_is_not_used_for_generation_or_execution() {
    let mut definition = model();
    definition["ddl"] = json!("DROP TABLE users; SELECT * FROM read_csv('/secret')");
    let result = action("data_model", &definition, "validate", &json!({}))
        .unwrap()
        .unwrap();
    assert_eq!(result["executed"], false);
    assert!(!result["generated_ddl"].as_str().unwrap().contains("DROP"));
    for operation in ["publish", "rollback"] {
        assert!(action("data_model", &definition, operation, &json!({})).is_err());
    }
}

#[test]
fn model_rejects_missing_or_duplicate_keys_and_case_colliding_columns() {
    let mut definition = model();
    definition["primary_key"] = json!(["unknown"]);
    assert!(validate("data_model", &definition).is_err());
    definition["primary_key"] = json!(["OrderId", "orderid"]);
    assert!(validate("data_model", &definition).is_err());
    definition["primary_key"] = json!([]);
    definition["columns"][1]["name"] = json!("orderid");
    assert!(validate("data_model", &definition).is_err());
}

#[test]
fn model_rejects_identifier_type_and_extra_column_injection() {
    for name in [
        "x\"; DROP TABLE users; --",
        "external.main.orders",
        "../orders",
        "orders;",
        " orders",
    ] {
        let mut definition = model();
        definition["table_name"] = json!(name);
        assert!(generate_ddl(&definition).is_err());
    }
    let mut definition = model();
    definition["columns"][0]["default"] = json!("getenv('TOKEN')");
    assert!(validate("data_model", &definition).is_err());
    let mut definition = model();
    definition["columns"][0]["data_type"] = json!("BIGINT); COPY users TO '/tmp/users'; --");
    assert!(validate("data_model", &definition).is_err());
}

#[test]
fn models_can_use_reserved_identifiers_because_ddl_quotes_them() {
    let mut definition = model();
    definition["table_name"] = json!("select");
    definition["columns"][1]["name"] = json!("from");
    let ddl = generate_ddl(&definition).unwrap();
    assert!(ddl.contains("CREATE TABLE \"select\""));
    assert!(ddl.contains("\"from\" DECIMAL(18,2)"));
    assert!(Parser::parse_sql(&DuckDbDialect {}, &ddl).is_ok());
}
