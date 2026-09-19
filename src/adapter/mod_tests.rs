use super::*;
use crate::{LogicalRequest, Query};
use secrecy::SecretString;
use std::time::Duration;

#[test]
fn factory_builds_the_compiled_in_typesafe_adapter() {
    let config = TypeSafeConfig::new(
        SecretString::from("test-key"),
        "jev-latest".to_owned(),
        Duration::from_secs(1),
    );
    let adapter = build_adapter(config).expect("factory should build TypeSafe adapter");
    let query = Query::try_new("query").expect("query should be valid");
    let request = LogicalRequest {
        query: &query,
        items: &[],
    };

    assert!(adapter.plan_batches(&request).is_ok());
}
