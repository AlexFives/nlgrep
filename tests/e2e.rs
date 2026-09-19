use nlgrep::{Probability, Query, RunConfig, TypeSafeAdapter, run_with_adapter};
use serde_json::{Value, json};
use std::{fs, io::Cursor, path::PathBuf, time::Duration};
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config(json_output: bool, all: bool, files: Vec<PathBuf>) -> RunConfig {
    RunConfig {
        query: Query::try_new("select fruit").expect("valid query"),
        threshold: Probability::try_from(0.5).expect("valid threshold"),
        json: json_output,
        all,
        model: "jev-latest".to_owned(),
        timeout: Duration::from_secs(30),
        concurrency: 4,
        files,
    }
}

fn success_body(probabilities: &[f64]) -> Value {
    let answers = probabilities
        .iter()
        .enumerate()
        .map(|(index, probability)| {
            (
                format!("item_{index}"),
                json!({"type": "noul", "noul": probability}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({
        "model": "jev-latest",
        "answers": answers,
        "usage": {"input_tokens": 10, "output_tokens": 4}
    })
}

async fn mount_success(server: &MockServer, probabilities: &[f64]) {
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_body(probabilities)))
        .expect(1)
        .mount(server)
        .await;
}

fn adapter(server: &MockServer) -> TypeSafeAdapter {
    TypeSafeAdapter::for_tests(server.uri(), "test-key").expect("test adapter should build")
}

#[tokio::test]
async fn classifies_file_lines_and_emits_matching_plain_text() {
    let directory = tempdir().expect("temporary directory should be created");
    let path = directory.path().join("words.txt");
    fs::write(&path, "car\nbanana\napple\n").expect("fixture should be written");
    let server = MockServer::start().await;
    mount_success(&server, &[0.1, 0.9, 0.95]).await;
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let outcome = run_with_adapter(
        &config(false, false, vec![path]),
        &adapter(&server),
        &mut input,
        &mut output,
        &mut diagnostics,
    )
    .await;

    assert_eq!(outcome.exit_code, 0);
    assert_eq!(output, b"banana\napple\n");
    assert!(diagnostics.is_empty());
}

#[tokio::test]
async fn emits_jsonl_for_all_stdin_decisions() {
    let server = MockServer::start().await;
    mount_success(&server, &[0.1, 0.9]).await;
    let mut input = Cursor::new(b"car\nbanana\n".to_vec());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let outcome = run_with_adapter(
        &config(true, true, Vec::new()),
        &adapter(&server),
        &mut input,
        &mut output,
        &mut diagnostics,
    )
    .await;

    let lines = String::from_utf8(output)
        .expect("JSONL output should be UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("each line should be JSON"))
        .collect::<Vec<_>>();
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["file"], Value::Null);
    assert_eq!(lines[0]["line"], 1);
    assert_eq!(lines[0]["matched"], false);
    assert_eq!(lines[1]["text"], "banana");
    assert_eq!(lines[1]["matched"], true);
    assert!(diagnostics.is_empty());
}

#[tokio::test]
async fn prefixes_plain_matches_when_multiple_files_are_selected() {
    let directory = tempdir().expect("temporary directory should be created");
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    fs::write(&first, "banana\n").expect("first fixture should be written");
    fs::write(&second, "apple\n").expect("second fixture should be written");
    let server = MockServer::start().await;
    mount_success(&server, &[0.9, 0.8]).await;
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let outcome = run_with_adapter(
        &config(false, false, vec![first.clone(), second.clone()]),
        &adapter(&server),
        &mut input,
        &mut output,
        &mut diagnostics,
    )
    .await;

    let expected = format!(
        "{}:{}\n{}:{}\n",
        first.display(),
        "banana",
        second.display(),
        "apple"
    );
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(
        String::from_utf8(output).expect("output should be UTF-8"),
        expected
    );
    assert!(diagnostics.is_empty());
}

#[tokio::test]
async fn malformed_and_unreadable_files_return_exit_two_but_readable_files_are_classified() {
    let directory = tempdir().expect("temporary directory should be created");
    let good = directory.path().join("good.txt");
    let missing = directory.path().join("missing.txt");
    let malformed = directory.path().join("malformed.txt");
    fs::write(&good, "banana\n").expect("fixture should be written");
    fs::write(&malformed, [0xff, b'\n']).expect("malformed fixture should be written");
    let server = MockServer::start().await;
    mount_success(&server, &[0.9]).await;
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let outcome = run_with_adapter(
        &config(
            false,
            false,
            vec![missing.clone(), malformed.clone(), good.clone()],
        ),
        &adapter(&server),
        &mut input,
        &mut output,
        &mut diagnostics,
    )
    .await;

    assert_eq!(outcome.exit_code, 2);
    assert_eq!(
        String::from_utf8(output).expect("output should be UTF-8"),
        format!("{}:banana\n", good.display())
    );
    let diagnostics = String::from_utf8(diagnostics).expect("diagnostics should be UTF-8");
    assert!(diagnostics.contains(&missing.display().to_string()));
    assert!(diagnostics.contains(&malformed.display().to_string()));
}

#[tokio::test]
async fn retries_transient_provider_failure_once_and_preserves_output() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_body(&[0.9])))
        .expect(1)
        .mount(&server)
        .await;
    let mut input = Cursor::new(b"banana\n".to_vec());
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();

    let outcome = run_with_adapter(
        &config(false, false, Vec::new()),
        &adapter(&server),
        &mut input,
        &mut output,
        &mut diagnostics,
    )
    .await;

    assert_eq!(outcome.exit_code, 0);
    assert_eq!(output, b"banana\n");
    assert_eq!(
        server.received_requests().await.expect("recording").len(),
        2
    );
    assert!(diagnostics.is_empty());
}
