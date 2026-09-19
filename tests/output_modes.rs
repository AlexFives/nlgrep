use nlgrep::{Decision, DecisionSink, LineEnding, OutputWriter, Probability, Source};
use serde_json::Value;
use std::path::PathBuf;

fn probability(value: f64) -> Probability {
    Probability::try_from(value).expect("fixture probability is valid")
}

fn fixture_decisions() -> Vec<Decision> {
    vec![
        Decision::new(
            Source::stdin(),
            1,
            "car",
            LineEnding::Lf,
            probability(0.03),
            false,
        ),
        Decision::new(
            Source::stdin(),
            2,
            "banana",
            LineEnding::CrLf,
            probability(0.98),
            true,
        ),
        Decision::new(
            Source::stdin(),
            3,
            "",
            LineEnding::Lf,
            probability(0.10),
            false,
        ),
        Decision::new(
            Source::stdin(),
            4,
            "apple",
            LineEnding::None,
            probability(0.99),
            true,
        ),
    ]
}

fn fixture_multiple_source_decisions() -> Vec<Decision> {
    vec![
        Decision::new(
            Source::file(PathBuf::from("a.txt")),
            2,
            "banana",
            LineEnding::Lf,
            probability(0.98),
            true,
        ),
        Decision::new(
            Source::file(PathBuf::from("b.txt")),
            1,
            "apple",
            LineEnding::Lf,
            probability(0.99),
            true,
        ),
    ]
}

fn render_json(decisions: &[Decision], all: bool) -> Vec<Value> {
    let mut output = Vec::new();
    OutputWriter::json(&mut output, all)
        .write(decisions)
        .expect("JSON output should succeed");
    String::from_utf8(output)
        .expect("JSONL is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is JSON"))
        .collect()
}

#[test]
fn text_mode_writes_only_matches_and_preserves_line_endings() {
    let decisions = fixture_decisions();
    let mut output = Vec::new();
    OutputWriter::text(&mut output, false)
        .write(&decisions)
        .expect("write should succeed");
    assert_eq!(output, b"banana\r\napple");
}

#[test]
fn multiple_files_prefix_text_matches_but_stdin_has_no_prefix() {
    let decisions = fixture_multiple_source_decisions();
    let mut output = Vec::new();
    OutputWriter::text(&mut output, true)
        .write(&decisions)
        .expect("write should succeed");
    assert_eq!(
        String::from_utf8(output).expect("UTF-8 output"),
        "a.txt:banana\nb.txt:apple\n"
    );
}

#[test]
fn json_mode_filters_and_json_all_emits_rejected_records() {
    let decisions = fixture_decisions();
    let filtered = render_json(&decisions, false);
    let all = render_json(&decisions, true);
    assert_eq!(filtered.len(), 2);
    assert_eq!(all.len(), decisions.len());
    assert_eq!(all[0]["matched"], false);
    assert_eq!(all[0]["text"], "car");
    assert_eq!(all[0]["file"], Value::Null);
}
