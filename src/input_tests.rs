use super::*;
use std::io::{self, Cursor, Read};
use tempfile::tempdir;

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("fixture read failure"))
    }
}

#[test]
fn source_and_line_ending_accessors_preserve_metadata() {
    let path = PathBuf::from("words.txt");
    assert_eq!(Source::stdin().path(), None);
    assert_eq!(Source::file(path.clone()).path(), Some(path.as_path()));
    assert_eq!(LineEnding::Lf.bytes(), b"\n");
    assert_eq!(LineEnding::CrLf.bytes(), b"\r\n");
    assert_eq!(LineEnding::None.bytes(), b"");
}

#[test]
fn repeated_stdin_is_reported_without_rereading_it() {
    let mut stdin = Cursor::new(b"banana\n".to_vec());
    let result = read_inputs(&[PathBuf::from("-"), PathBuf::from("-")], &mut stdin);

    assert_eq!(result.records.len(), 1);
    assert!(matches!(
        result.errors.as_slice(),
        [InputError::RepeatedStdin]
    ));
}

#[test]
fn stdin_read_failure_is_preserved_as_an_input_error() {
    let mut stdin = FailingReader;
    let result = read_inputs(&[], &mut stdin);

    assert!(result.records.is_empty());
    assert!(matches!(
        result.errors.as_slice(),
        [InputError::Read { .. }]
    ));
}

#[test]
fn malformed_file_is_reported_without_discarding_other_files() {
    let directory = tempdir().expect("temporary directory should be created");
    let malformed = directory.path().join("bad.txt");
    let good = directory.path().join("good.txt");
    std::fs::write(&malformed, [0xff, b'\n']).expect("bad fixture should be written");
    std::fs::write(&good, b"banana\n").expect("good fixture should be written");
    let mut stdin = Cursor::new(Vec::<u8>::new());

    let result = read_inputs(&[malformed.clone(), good.clone()], &mut stdin);

    assert_eq!(result.records.len(), 1);
    assert_eq!(result.records[0].source, Source::file(good));
    assert!(matches!(
        result.errors.as_slice(),
        [InputError::InvalidUtf8 { .. }]
    ));
    assert!(
        result.errors[0]
            .to_string()
            .contains(&malformed.display().to_string())
    );
}

#[test]
fn read_bytes_reports_invalid_utf8() {
    let error = read_bytes(&[0xff]).expect_err("invalid UTF-8 should be rejected");

    assert!(matches!(error, InputError::InvalidUtf8 { offset: 0, .. }));
}

#[test]
fn read_bytes_preserves_crlf_and_unterminated_line_endings() {
    let records = read_bytes(b"banana\r\napple").expect("fixture input should be valid");

    assert_eq!(records[0].line_ending, LineEnding::CrLf);
    assert_eq!(records[1].line_ending, LineEnding::None);
}

#[test]
fn missing_file_is_reported_as_a_read_error() {
    let directory = tempdir().expect("temporary directory should be created");
    let missing = directory.path().join("missing.txt");
    let mut stdin = Cursor::new(Vec::<u8>::new());

    let result = read_inputs(std::slice::from_ref(&missing), &mut stdin);

    assert!(result.records.is_empty());
    assert!(matches!(
        result.errors.as_slice(),
        [InputError::Read { .. }]
    ));
}

#[test]
fn record_id_overflow_is_rejected_at_the_parse_boundary() {
    let mut next_id = u64::MAX;
    let error = parse_line(b"last", false, Source::stdin(), 1, &mut next_id)
        .expect_err("record IDs must not wrap");

    assert!(matches!(error, InputError::TooManyRecords));
}
