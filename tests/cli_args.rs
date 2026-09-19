use clap::Parser;
use nlgrep::{Cli, RunConfig};

#[test]
fn parses_query_threshold_json_all_timeout_and_concurrency() {
    let cli = Cli::try_parse_from([
        "nlgrep",
        "--threshold",
        "0.8",
        "--json",
        "--all",
        "--timeout",
        "30s",
        "--concurrency",
        "4",
        "select fruit",
        "words.txt",
    ])
    .expect("valid arguments");

    assert_eq!(cli.query, "select fruit");
    assert!(cli.json);
    assert!(cli.all);
    assert_eq!(cli.concurrency, 4);
}

#[test]
fn accepts_zero_as_unlimited_concurrency_and_rejects_negative_values() {
    let cli = Cli::try_parse_from(["nlgrep", "--concurrency", "0", "select fruit"])
        .expect("zero is the unlimited mode");
    assert_eq!(cli.concurrency, 0);
    assert!(Cli::try_parse_from(["nlgrep", "--concurrency", "-1", "select fruit"]).is_err());
}

#[test]
fn rejects_all_without_json_and_empty_query() {
    let all_without_json =
        Cli::try_parse_from(["nlgrep", "--all", "query"]).expect("argument shape is valid");
    assert!(RunConfig::try_from(all_without_json).is_err());
    let empty_query = Cli::try_parse_from(["nlgrep", ""]).expect("argument shape is valid");
    assert!(RunConfig::try_from(empty_query).is_err());
}
