use super::*;
use crate::{Candidate, CandidateId};
use std::io::{self, Write};
use std::path::PathBuf;

fn decision(source: Source, matched: bool) -> Decision {
    Decision::new(
        source,
        2,
        "banana",
        LineEnding::Lf,
        crate::Probability::try_from(0.8).expect("fixture probability is valid"),
        matched,
    )
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("fixture write failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct FailAtWrite {
    writes: usize,
    fail_at: usize,
}

impl Write for FailAtWrite {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.writes += 1;
        if self.writes == self.fail_at {
            Err(io::Error::other("fixture write failure"))
        } else {
            Ok(buffer.len())
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct FailOnNewline;

impl Write for FailOnNewline {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer == b"\n" {
            Err(io::Error::other("fixture newline failure"))
        } else {
            Ok(buffer.len())
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn decision_constructor_and_output_modes_are_typed() {
    let candidate = Candidate::new(CandidateId::new(1), "banana");
    let output = decision(Source::stdin(), true);

    assert_eq!(candidate.id.value(), 1);
    assert_eq!(output.text, "banana");
    assert_eq!(OutputMode::Plain, OutputMode::Plain);
    assert_eq!(
        OutputMode::Json { all: true },
        OutputMode::Json { all: true }
    );
}

#[test]
fn text_output_reports_non_utf8_paths() {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(vec![0xff]));
        let mut writer = OutputWriter::text(Vec::new(), true);

        let error = writer
            .write(&[decision(Source::file(path), true)])
            .expect_err("non-UTF-8 path must fail");

        assert!(matches!(error, OutputError::NonUtf8Path { .. }));
    }
}

#[test]
fn json_output_reports_non_utf8_paths() {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(vec![0xff]));
        let mut writer = OutputWriter::json(Vec::new(), true);

        let error = writer
            .write(&[decision(Source::file(path), true)])
            .expect_err("non-UTF-8 path must fail");

        assert!(matches!(error, OutputError::NonUtf8Path { .. }));
    }
}

#[test]
fn output_writers_preserve_io_errors() {
    let mut text_writer = OutputWriter::text(FailingWriter, false);
    let text_error = text_writer
        .write(&[decision(Source::stdin(), true)])
        .expect_err("text write should fail");
    assert!(matches!(text_error, OutputError::Io(_)));

    let mut json_writer = OutputWriter::json(FailingWriter, false);
    let json_error = json_writer
        .write(&[decision(Source::stdin(), true)])
        .expect_err("JSON serialization should fail at the writer");
    assert!(matches!(json_error, OutputError::Json(_)));
}

#[test]
fn text_output_preserves_errors_from_each_path_prefix_write() {
    for fail_at in [1, 2, 4] {
        let mut writer = OutputWriter::text(FailAtWrite { writes: 0, fail_at }, true);
        let error = writer
            .write(&[decision(Source::file(PathBuf::from("words.txt")), true)])
            .expect_err("fixture writer should fail");

        assert!(matches!(error, OutputError::Io(_)));
    }
}

#[test]
fn json_output_preserves_newline_write_errors() {
    let mut writer = OutputWriter::json(FailOnNewline, false);
    let error = writer
        .write(&[decision(Source::stdin(), true)])
        .expect_err("newline write should fail");

    assert!(matches!(error, OutputError::Io(_)));
}
