use nlgrep::{LineEnding, read_bytes};

#[test]
fn preserves_crlf_and_missing_final_line_terminator() {
    let records = read_bytes(b"car\r\nbanana").expect("valid UTF-8");
    assert_eq!(records[0].line_ending, LineEnding::CrLf);
    assert_eq!(records[0].candidate.text, "car");
    assert_eq!(records[1].line_ending, LineEnding::None);
}

#[test]
fn does_not_create_synthetic_record_after_final_lf() {
    let records = read_bytes(b"car\n\nbanana\n").expect("valid UTF-8");
    assert_eq!(records.len(), 3);
    assert_eq!(records[1].candidate.text, "");
    assert_eq!(records[2].candidate.text, "banana");
}

#[test]
fn duplicate_lines_keep_distinct_ids() {
    let records = read_bytes(b"same\nsame").expect("valid UTF-8");
    assert_eq!(records[0].candidate.text, records[1].candidate.text);
    assert_ne!(records[0].candidate.id, records[1].candidate.id);
}

#[test]
fn rejects_malformed_utf8_without_replacement() {
    assert!(read_bytes(b"valid\xff").is_err());
}
