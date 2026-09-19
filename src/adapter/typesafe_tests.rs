use super::*;
use crate::{BatchRequest, Candidate, CandidateId, ModelAdapter, Query};
use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request_with(items: usize) -> (Query, Vec<Candidate>) {
    let query = Query::try_new("select fruit").expect("query should be valid");
    let candidates = (0..items)
        .map(|id| Candidate::new(CandidateId::new(id as u64), format!("item-{id}")))
        .collect();
    (query, candidates)
}

#[test]
fn plans_batches_from_context_budget_instead_of_fixed_item_count() {
    let adapter =
        TypeSafeAdapter::for_tests("http://127.0.0.1:1", "test-key").expect("adapter should build");
    let query = Query::try_new("select fruit").expect("query should be valid");
    let candidates = (0..10_000)
        .map(|id| Candidate::new(CandidateId::new(id), format!("record-{id}")))
        .collect::<Vec<_>>();
    let request = crate::LogicalRequest {
        query: &query,
        items: &candidates,
    };

    let plan = adapter
        .plan_batches(&request)
        .expect("batch planning should succeed");

    assert!(plan.ranges.first().expect("plan should not be empty").len() > 32);
    assert!(plan.ranges.len() <= 8);
    assert!(plan.ranges.len() < 313);
    assert!(plan.validate(candidates.len()).is_ok());
}

#[test]
fn starts_a_new_batch_when_the_state_budget_would_be_exceeded() {
    let adapter =
        TypeSafeAdapter::for_tests("http://127.0.0.1:1", "test-key").expect("adapter should build");
    let query = Query::try_new("select fruit").expect("query should be valid");
    let candidates = (0..3)
        .map(|id| Candidate::new(CandidateId::new(id), "x".repeat(50_000)))
        .collect::<Vec<_>>();
    let request = crate::LogicalRequest {
        query: &query,
        items: &candidates,
    };

    let plan = adapter
        .plan_batches(&request)
        .expect("batch planning should succeed");

    assert_eq!(plan.ranges, vec![0..1, 1..2, 2..3]);
}

#[test]
fn uses_a_more_conservative_estimate_for_non_ascii_state() {
    let adapter =
        TypeSafeAdapter::for_tests("http://127.0.0.1:1", "test-key").expect("adapter should build");
    let query = Query::try_new("select fruit").expect("query should be valid");
    let candidates = (0..3)
        .map(|id| Candidate::new(CandidateId::new(id), "ж".repeat(20_000)))
        .collect::<Vec<_>>();
    let request = crate::LogicalRequest {
        query: &query,
        items: &candidates,
    };

    let plan = adapter
        .plan_batches(&request)
        .expect("batch planning should succeed");

    assert_eq!(plan.ranges, vec![0..1, 1..2, 2..3]);
}

fn success_body(items: usize) -> serde_json::Value {
    let answers = (0..items)
        .map(|index| {
            (
                format!("item_{index}"),
                json!({"type": "noul", "noul": 0.8}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({
        "model": "jev-latest",
        "answers": answers,
        "usage": {"input_tokens": 1, "output_tokens": 1}
    })
}

async fn classify(
    adapter: &TypeSafeAdapter,
    query: &Query,
    candidates: &[Candidate],
) -> Result<BatchResponse, AdapterError> {
    adapter
        .classify_batch(&BatchRequest {
            query,
            items: candidates,
        })
        .await
}

#[tokio::test]
async fn invalid_json_response_is_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{"))
        .mount(&server)
        .await;
    let adapter =
        TypeSafeAdapter::for_tests(server.uri(), "test-key").expect("adapter should build");
    let (query, candidates) = request_with(1);

    let error = classify(&adapter, &query, &candidates)
        .await
        .expect_err("malformed JSON should be rejected");

    assert!(matches!(error, AdapterError::InvalidResponse(_)));
}

#[tokio::test]
async fn missing_question_id_is_rejected_after_answer_count_check() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "jev-latest",
            "answers": {
                "item_0": {"type": "noul", "noul": 0.8},
                "item_2": {"type": "noul", "noul": 0.8}
            },
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .mount(&server)
        .await;
    let adapter =
        TypeSafeAdapter::for_tests(server.uri(), "test-key").expect("adapter should build");
    let (query, candidates) = request_with(2);

    let error = classify(&adapter, &query, &candidates)
        .await
        .expect_err("missing question ID should be rejected");

    assert!(matches!(error, AdapterError::InvalidResponse(_)));
}

#[tokio::test]
async fn missing_noul_probability_is_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "jev-latest",
            "answers": {"item_0": {"type": "noul"}},
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .mount(&server)
        .await;
    let adapter =
        TypeSafeAdapter::for_tests(server.uri(), "test-key").expect("adapter should build");
    let (query, candidates) = request_with(1);

    let error = classify(&adapter, &query, &candidates)
        .await
        .expect_err("missing noul should be rejected");

    assert!(matches!(error, AdapterError::InvalidResponse(_)));
}

#[tokio::test]
async fn provider_rejection_is_preserved_without_retry() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(422))
        .mount(&server)
        .await;
    let adapter =
        TypeSafeAdapter::for_tests(server.uri(), "test-key").expect("adapter should build");
    let (query, candidates) = request_with(1);

    let error = classify(&adapter, &query, &candidates)
        .await
        .expect_err("provider rejection should be returned");

    assert!(matches!(
        error,
        AdapterError::ProviderRejected { status: 422, .. }
    ));
    assert_eq!(
        server.received_requests().await.expect("recording").len(),
        1
    );
}

#[tokio::test]
async fn overloaded_provider_is_retried_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(529))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_body(1)))
        .mount(&server)
        .await;
    let adapter =
        TypeSafeAdapter::for_tests(server.uri(), "test-key").expect("adapter should build");
    let (query, candidates) = request_with(1);

    let response = classify(&adapter, &query, &candidates)
        .await
        .expect("529 should be retried");

    assert_eq!(response.judgments.len(), 1);
    assert_eq!(
        server.received_requests().await.expect("recording").len(),
        2
    );
}

#[tokio::test]
async fn connection_failure_becomes_transport_error_after_retry() {
    let adapter =
        TypeSafeAdapter::for_tests("http://127.0.0.1:1", "test-key").expect("adapter should build");
    let (query, candidates) = request_with(1);

    let error = classify(&adapter, &query, &candidates)
        .await
        .expect_err("connection failure should be returned");

    assert!(matches!(error, AdapterError::Transport(_)));
}

#[tokio::test]
async fn response_timeout_becomes_timeout_error_after_retry() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(100))
                .set_body_json(success_body(1)),
        )
        .mount(&server)
        .await;
    let mut config = TypeSafeConfig::for_tests(server.uri(), "test-key");
    config.timeout = Duration::from_millis(1);
    let adapter = TypeSafeAdapter::new(config).expect("adapter should build");
    let (query, candidates) = request_with(1);

    let error = classify(&adapter, &query, &candidates)
        .await
        .expect_err("timeout should be returned");

    assert!(matches!(error, AdapterError::Timeout));
}

#[test]
fn client_builder_failure_is_mapped_to_configuration_error() {
    let config = TypeSafeConfig::for_tests("http://127.0.0.1:1", "test-key");
    let client_error = reqwest::Proxy::all("http://[").expect_err("proxy URL should be invalid");

    let result = TypeSafeAdapter::from_client_result(config, Err(client_error));

    assert!(matches!(result, Err(AdapterError::Configuration(_))));
}

#[tokio::test]
async fn duplicate_candidate_ids_are_rejected_after_response_parsing() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_body(2)))
        .mount(&server)
        .await;
    let adapter =
        TypeSafeAdapter::for_tests(server.uri(), "test-key").expect("adapter should build");
    let query = Query::try_new("select fruit").expect("query should be valid");
    let candidates = vec![
        Candidate::new(CandidateId::new(0), "banana"),
        Candidate::new(CandidateId::new(0), "apple"),
    ];

    let error = classify(&adapter, &query, &candidates)
        .await
        .expect_err("duplicate candidate IDs should be rejected");

    assert!(matches!(error, AdapterError::InvalidResponse(_)));
}

#[tokio::test]
async fn response_validation_rejects_count_type_and_probability_errors() {
    let bodies = [
        json!({
            "model": "jev-latest",
            "answers": {},
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        json!({
            "model": "jev-latest",
            "answers": {"item_0": {"type": "choice", "noul": 0.8}},
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
        json!({
            "model": "jev-latest",
            "answers": {"item_0": {"type": "noul", "noul": -0.1}},
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }),
    ];

    for body in bodies {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let adapter =
            TypeSafeAdapter::for_tests(server.uri(), "test-key").expect("adapter should build");
        let (query, candidates) = request_with(1);

        let error = classify(&adapter, &query, &candidates)
            .await
            .expect_err("invalid response should be rejected");

        assert!(matches!(error, AdapterError::InvalidResponse(_)));
    }
}

#[tokio::test]
async fn unauthorized_provider_response_becomes_authentication_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let adapter =
        TypeSafeAdapter::for_tests(server.uri(), "test-key").expect("adapter should build");
    let (query, candidates) = request_with(1);

    let error = classify(&adapter, &query, &candidates)
        .await
        .expect_err("authentication response should fail");

    assert!(matches!(error, AdapterError::Authentication));
}

#[test]
fn retry_predicates_distinguish_transient_statuses() {
    let adapter =
        TypeSafeAdapter::for_tests("http://127.0.0.1:1", "test-key").expect("adapter should build");

    assert!(adapter.should_retry_status(StatusCode::TOO_MANY_REQUESTS, 0));
    assert!(adapter.should_retry_status(StatusCode::from_u16(529).expect("529 is valid"), 0));
    assert!(!adapter.should_retry_status(StatusCode::INTERNAL_SERVER_ERROR, 0));
    assert!(!adapter.should_retry_status(StatusCode::TOO_MANY_REQUESTS, 1));
}
