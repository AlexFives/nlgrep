use crate::{Probability, Source, error::OutputError, input::LineEnding};
use serde::Serialize;
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub source: Source,
    pub line_number: usize,
    pub text: String,
    pub line_ending: LineEnding,
    pub probability: Probability,
    pub matched: bool,
}

impl Decision {
    pub fn new(
        source: Source,
        line_number: usize,
        text: impl Into<String>,
        line_ending: LineEnding,
        probability: Probability,
        matched: bool,
    ) -> Self {
        Self {
            source,
            line_number,
            text: text.into(),
            line_ending,
            probability,
            matched,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Plain,
    Json { all: bool },
}

pub trait DecisionSink: Send {
    fn write(&mut self, decisions: &[Decision]) -> Result<(), OutputError>;
}

pub struct OutputWriter<W> {
    writer: W,
    mode: OutputMode,
    multiple_files: bool,
}

impl<W: Write> OutputWriter<W> {
    pub fn text(writer: W, multiple_files: bool) -> Self {
        Self {
            writer,
            mode: OutputMode::Plain,
            multiple_files,
        }
    }

    pub fn json(writer: W, all: bool) -> Self {
        Self {
            writer,
            mode: OutputMode::Json { all },
            multiple_files: false,
        }
    }
}

impl<W: Write + Send> DecisionSink for OutputWriter<W> {
    fn write(&mut self, decisions: &[Decision]) -> Result<(), OutputError> {
        let mut core = OutputWriterCore {
            writer: &mut self.writer,
            mode: self.mode,
            multiple_files: self.multiple_files,
        };
        core.write(decisions)
    }
}

struct OutputWriterCore<'a> {
    writer: &'a mut dyn Write,
    mode: OutputMode,
    multiple_files: bool,
}

impl OutputWriterCore<'_> {
    fn write(&mut self, decisions: &[Decision]) -> Result<(), OutputError> {
        match self.mode {
            OutputMode::Plain => self.write_plain(decisions),
            OutputMode::Json { all } => self.write_json(decisions, all),
        }
    }

    fn write_plain(&mut self, decisions: &[Decision]) -> Result<(), OutputError> {
        for decision in decisions.iter().filter(|decision| decision.matched) {
            if self.multiple_files
                && let Some(path) = decision.source.path()
            {
                let path_text = path.to_str().ok_or_else(|| OutputError::NonUtf8Path {
                    path: path.to_string_lossy().into_owned(),
                })?;
                self.writer.write_all(path_text.as_bytes())?;
                self.writer.write_all(b":")?;
            }
            self.writer.write_all(decision.text.as_bytes())?;
            self.writer.write_all(decision.line_ending.bytes())?;
        }
        Ok(())
    }

    fn write_json(&mut self, decisions: &[Decision], all: bool) -> Result<(), OutputError> {
        for decision in decisions.iter().filter(|decision| all || decision.matched) {
            let file = decision
                .source
                .path()
                .map(|path| {
                    path.to_str()
                        .map(str::to_owned)
                        .ok_or_else(|| OutputError::NonUtf8Path {
                            path: path.to_string_lossy().into_owned(),
                        })
                })
                .transpose()?;
            let record = JsonDecision {
                file,
                line: decision.line_number,
                text: &decision.text,
                matched: decision.matched,
                probability: decision.probability.get(),
            };
            serde_json::to_writer(&mut self.writer, &record)?;
            self.writer.write_all(b"\n")?;
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct JsonDecision<'a> {
    file: Option<String>,
    line: usize,
    text: &'a str,
    matched: bool,
    probability: f64,
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
