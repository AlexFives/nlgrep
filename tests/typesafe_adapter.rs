use nlgrep::{
    BatchRequest, Candidate, CandidateId, LogicalRequest, ModelAdapter, Query, TypeSafeAdapter,
};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn success_body() -> Value {
    json!({
        "model": "jev-latest",
        "answers": {
            "item_0": {"type": "noul", "noul": 0.03},
            "item_1": {"type": "noul", "noul": 0.98}
        },
        "usage": {"input_tokens": 10, "output_tokens": 4}
    })
}

async fn mount_success_response(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_body()))
        .expect(1)
        .mount(server)
        .await;
}

fn test_adapter(server: &MockServer) -> TypeSafeAdapter {
    TypeSafeAdapter::for_tests(server.uri(), "test-key").expect("test adapter should build")
}

fn test_adapter_without_server() -> TypeSafeAdapter {
    TypeSafeAdapter::for_tests("http://127.0.0.1:1/v1/systemone", "test-key")
        .expect("test adapter should build")
}

fn fixture_query_and_candidates() -> (Query, Vec<Candidate>) {
    let query = Query::try_new("select fruit").expect("valid query");
    let candidates = vec![
        Candidate::new(CandidateId::new(0), "car"),
        Candidate::new(CandidateId::new(1), "banana"),
    ];
    (query, candidates)
}

#[tokio::test]
async fn sends_query_items_noul_questions_and_auth_header() {
    let server = MockServer::start().await;
    mount_success_response(&server).await;
    let adapter = test_adapter(&server);
    let (query, candidates) = fixture_query_and_candidates();
    let batch = BatchRequest {
        query: &query,
        items: &candidates,
    };

    let response = adapter
        .classify_batch(&batch)
        .await
        .expect("request succeeds");

    assert_eq!(response.judgments.len(), 2);
    let request = server
        .received_requests()
        .await
        .expect("request recording is enabled")
        .pop()
        .expect("one request was sent");
    assert_eq!(request.method, wiremock::http::Method::POST);
    assert_eq!(request.url.path(), "/v1/systemone");
    assert_eq!(request.headers["authorization"], "Bearer test-key");
    let body: Value = serde_json::from_slice(&request.body).expect("request body is JSON");
    assert_eq!(body["model"], "jev-latest");
    assert_eq!(body["state"]["query"], "select fruit");
    assert_eq!(
        body["state"]["items"],
        json!([{"text": "car"}, {"text": "banana"}])
    );
    assert_eq!(body["questions"]["item_0"]["type"], "noul");
    assert_eq!(body["questions"]["item_1"]["type"], "noul");
}

#[test]
fn plans_contiguous_ranges_with_the_adapter_batch_limit() {
    let adapter = test_adapter_without_server();
    let query = Query::try_new("select fruit").expect("valid query");
    let candidates: Vec<_> = (0..65)
        .map(|index| Candidate::new(CandidateId::new(index), "item"))
        .collect();
    let request = LogicalRequest {
        query: &query,
        items: &candidates,
    };
    let plan = adapter.plan_batches(&request).expect("plan succeeds");
    assert_eq!(plan.ranges, vec![0..32, 32..64, 64..65]);
}

#[tokio::test]
async fn retries_429_once_with_backoff_then_returns_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_body()))
        .mount(&server)
        .await;
    let adapter = test_adapter(&server);
    let (query, candidates) = fixture_query_and_candidates();
    let batch = BatchRequest {
        query: &query,
        items: &candidates,
    };

    let response = adapter
        .classify_batch(&batch)
        .await
        .expect("retry succeeds");

    assert_eq!(response.judgments.len(), 2);
    assert_eq!(
        server.received_requests().await.expect("recording").len(),
        2
    );
}

#[tokio::test]
async fn does_not_retry_authentication_or_validation_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    let adapter = test_adapter(&server);
    let (query, candidates) = fixture_query_and_candidates();
    let batch = BatchRequest {
        query: &query,
        items: &candidates,
    };

    let error = adapter
        .classify_batch(&batch)
        .await
        .expect_err("authentication errors are not retried");

    assert!(matches!(error, nlgrep::AdapterError::Authentication));
}

#[tokio::test]
async fn rejects_wrong_answer_type_missing_id_duplicate_id_and_invalid_probability() {
    let invalid_bodies = [
        json!({"model":"jev-latest","answers":{"item_0":{"type":"choice","noul":0.1},"item_1":{"type":"noul","noul":0.9}},"usage":{"input_tokens":1,"output_tokens":1}}),
        json!({"model":"jev-latest","answers":{"item_0":{"type":"noul","noul":0.1}},"usage":{"input_tokens":1,"output_tokens":1}}),
        json!({"model":"jev-latest","answers":{"item_0":{"type":"noul","noul":0.1},"item_1":{"type":"noul","noul":0.2},"item_2":{"type":"noul","noul":0.3}},"usage":{"input_tokens":1,"output_tokens":1}}),
        json!({"model":"jev-latest","answers":{"item_0":{"type":"noul","noul":-0.1},"item_1":{"type":"noul","noul":0.9}},"usage":{"input_tokens":1,"output_tokens":1}}),
    ];

    for body in invalid_bodies {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let adapter = test_adapter(&server);
        let (query, candidates) = fixture_query_and_candidates();
        let batch = BatchRequest {
            query: &query,
            items: &candidates,
        };

        let error = adapter
            .classify_batch(&batch)
            .await
            .expect_err("invalid response must be rejected");
        assert!(matches!(error, nlgrep::AdapterError::InvalidResponse(_)));
    }
}
