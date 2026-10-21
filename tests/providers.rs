use assert_cmd::Command;
use std::{fs, path::Path, process::Output};
use tempfile::TempDir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, header, method, path},
};

fn command(root: &Path) -> Command {
    let mut command = Command::cargo_bin("nlgrep").expect("binary should be built");
    command
        .current_dir(root)
        .env("XDG_CONFIG_HOME", root.join("global"))
        .env("HOME", root.join("home"))
        .env_remove("TYPESAFE_API_KEY");
    command
}

async fn run(mut command: Command) -> Output {
    tokio::task::spawn_blocking(move || command.output().expect("CLI should run"))
        .await
        .expect("CLI task should finish")
}

async fn expect_request(server: &MockServer, model: &str, key: &str) {
    Mock::given(method("POST"))
        .and(path("/custom/systemone"))
        .and(header("authorization", format!("Bearer {key}")))
        .and(body_partial_json(serde_json::json!({"model": model})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "answers": {"item_0": {"type": "noul", "noul": 0.9}},
            "usage": {"input_tokens": 10, "output_tokens": 1}
        })))
        .expect(1)
        .mount(server)
        .await;
}

fn provider(endpoint: &str, model: &str, key: &str) -> String {
    format!("endpoint = '{endpoint}/custom/systemone'\nmodel = '{model}'\napi_key = '{key}'\n")
}

#[tokio::test]
async fn configured_default_provider_routes_the_real_cli_request() {
    let root = TempDir::new().expect("temporary directory");
    let server = MockServer::start().await;
    expect_request(&server, "jev-private", "private-key").await;
    fs::write(
        root.path().join(".nlgrep.toml"),
        format!(
            "default_provider = 'private'\n[providers.private]\n{}",
            provider(&server.uri(), "jev-private", "private-key")
        ),
    )
    .expect("write project configuration");
    let mut cli = command(root.path());
    cli.arg("select fruit").write_stdin("banana\n");

    let output = run(cli).await;

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"banana\n");
    assert!(output.stderr.is_empty());
}

#[tokio::test]
async fn provider_and_model_flags_override_the_configured_defaults() {
    let root = TempDir::new().expect("temporary directory");
    let server = MockServer::start().await;
    expect_request(&server, "jev-cli", "backup-key").await;
    fs::write(
        root.path().join(".nlgrep.toml"),
        format!(
            "default_provider = 'primary'\n[providers.primary]\n{}\n[providers.backup]\n{}",
            provider("http://127.0.0.1:1", "jev-primary", "primary-key"),
            provider(&server.uri(), "jev-backup", "backup-key")
        ),
    )
    .expect("write project configuration");
    let mut cli = command(root.path());
    cli.args(["--provider", "backup", "--model", "jev-cli", "select fruit"])
        .write_stdin("banana\n");

    let output = run(cli).await;

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"banana\n");
}

#[tokio::test]
async fn provider_flag_selects_the_providers_model_without_a_model_flag() {
    let root = TempDir::new().expect("temporary directory");
    let server = MockServer::start().await;
    expect_request(&server, "jev-backup", "backup-key").await;
    fs::write(
        root.path().join(".nlgrep.toml"),
        format!(
            "[providers.backup]\n{}",
            provider(&server.uri(), "jev-backup", "backup-key")
        ),
    )
    .expect("write project configuration");
    let mut cli = command(root.path());
    cli.args(["--provider", "backup", "select fruit"])
        .write_stdin("banana\n");

    let output = run(cli).await;

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"banana\n");
}

#[tokio::test]
async fn project_provider_replaces_the_same_global_provider() {
    let root = TempDir::new().expect("temporary directory");
    let server = MockServer::start().await;
    expect_request(&server, "jev-project", "project-key").await;
    let global = root.path().join("global/nlgrep");
    fs::create_dir_all(&global).expect("create global configuration directory");
    fs::write(
        global.join("config.toml"),
        format!(
            "default_provider = 'shared'\n[providers.shared]\n{}",
            provider("http://127.0.0.1:1", "jev-global", "global-key")
        ),
    )
    .expect("write global configuration");
    fs::write(
        root.path().join(".nlgrep.toml"),
        format!(
            "[providers.shared]\n{}",
            provider(&server.uri(), "jev-project", "project-key")
        ),
    )
    .expect("write project configuration");
    let mut cli = command(root.path());
    cli.arg("select fruit").write_stdin("banana\n");

    let output = run(cli).await;

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"banana\n");
}

#[tokio::test]
async fn project_default_can_select_a_provider_from_the_global_configuration() {
    let root = TempDir::new().expect("temporary directory");
    let server = MockServer::start().await;
    expect_request(&server, "jev-global", "global-key").await;
    let global = root.path().join("global/nlgrep");
    fs::create_dir_all(&global).expect("create global configuration directory");
    fs::write(
        global.join("config.toml"),
        format!(
            "default_provider = 'unused'\n[providers.shared]\n{}",
            provider(&server.uri(), "jev-global", "global-key")
        ),
    )
    .expect("write global configuration");
    fs::write(
        root.path().join(".nlgrep.toml"),
        "default_provider = 'shared'\n",
    )
    .expect("write project configuration");
    let mut cli = command(root.path());
    cli.arg("select fruit").write_stdin("banana\n");

    let output = run(cli).await;

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"banana\n");
}

#[tokio::test]
async fn explicit_configuration_replaces_discovery_and_uses_a_provider_specific_key() {
    let root = TempDir::new().expect("temporary directory");
    let server = MockServer::start().await;
    expect_request(&server, "jev-env", "environment-key").await;
    fs::write(root.path().join(".nlgrep.toml"), "invalid TOML")
        .expect("write ignored project configuration");
    fs::write(
        root.path().join("custom.toml"),
        format!(
            "default_provider = 'custom'\n[providers.custom]\nendpoint = '{}/custom/systemone'\nmodel = 'jev-env'\napi_key_env = 'NLGREP_TEST_KEY'\n",
            server.uri()
        ),
    )
    .expect("write explicit configuration");
    let mut cli = command(root.path());
    cli.env("NLGREP_TEST_KEY", "environment-key")
        .args(["--config", "custom.toml", "select fruit"])
        .write_stdin("banana\n");

    let output = run(cli).await;

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"banana\n");
}

#[test]
fn unknown_provider_is_a_configuration_error_even_with_empty_input() {
    let root = TempDir::new().expect("temporary directory");

    command(root.path())
        .args(["--provider", "missing", "select fruit"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("missing"));
}

#[test]
fn missing_explicit_configuration_is_an_error() {
    let root = TempDir::new().expect("temporary directory");

    command(root.path())
        .args(["--config", "missing.toml", "select fruit"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("missing.toml"));
}

#[test]
fn invalid_configuration_does_not_expose_inline_keys() {
    let root = TempDir::new().expect("temporary directory");
    fs::write(
        root.path().join(".nlgrep.toml"),
        "[providers.custom]\napi_key = 'sensitive-test-key' unexpected\n",
    )
    .expect("write malformed configuration");

    let output = command(root.path())
        .arg("select fruit")
        .output()
        .expect("run CLI");

    assert_eq!(output.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("sensitive-test-key"));
}
