use super::*;
use clap::Parser;

#[test]
fn duration_parser_accepts_human_duration_and_rejects_invalid_text() {
    assert_eq!(
        parse_duration("250ms").expect("duration should parse"),
        Duration::from_millis(250)
    );
    assert!(parse_duration("not-a-duration").is_err());
}

#[test]
fn run_config_preserves_cli_values() {
    let cli = Cli::try_parse_from([
        "nlgrep",
        "--threshold",
        "0.8",
        "--json",
        "--all",
        "--model",
        "jev-custom",
        "--timeout",
        "5s",
        "--concurrency",
        "0",
        "query",
        "words.txt",
    ])
    .expect("arguments should parse");
    let config = RunConfig::try_from(cli).expect("configuration should be valid");

    assert_eq!(config.query.as_str(), "query");
    assert_eq!(config.threshold.get(), 0.8);
    assert!(config.json);
    assert!(config.all);
    assert_eq!(config.model, "jev-custom");
    assert_eq!(config.timeout, Duration::from_secs(5));
    assert_eq!(config.concurrency, 0);
    assert_eq!(config.files, vec![PathBuf::from("words.txt")]);
}
