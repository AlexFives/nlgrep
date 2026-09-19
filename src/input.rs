use crate::{Candidate, CandidateId};
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    str,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Stdin,
    File(PathBuf),
}

impl Source {
    pub fn stdin() -> Self {
        Self::Stdin
    }

    pub fn file(path: PathBuf) -> Self {
        Self::File(path)
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Stdin => None,
            Self::File(path) => Some(path),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Stdin => "stdin".to_owned(),
            Self::File(path) => path.to_string_lossy().into_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
    None,
}

impl LineEnding {
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Lf => b"\n",
            Self::CrLf => b"\r\n",
            Self::None => b"",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputRecord {
    pub candidate: Candidate,
    pub source: Source,
    pub line_number: usize,
    pub line_ending: LineEnding,
}

#[derive(Debug, Error)]
pub enum InputError {
    #[error("could not read {input_source}: {message}")]
    Read {
        input_source: String,
        message: String,
    },
    #[error("{input_source} is not valid UTF-8 at byte {offset}")]
    InvalidUtf8 { input_source: String, offset: usize },
    #[error("stdin was requested more than once")]
    RepeatedStdin,
    #[error("input contains more than u64::MAX records")]
    TooManyRecords,
}

#[derive(Debug)]
pub struct InputReadResult {
    pub records: Vec<InputRecord>,
    pub errors: Vec<InputError>,
}

pub fn read_bytes(bytes: &[u8]) -> Result<Vec<InputRecord>, InputError> {
    let mut next_id = 0;
    parse_bytes(bytes, Source::stdin(), &mut next_id)
}

pub fn read_inputs(files: &[PathBuf], stdin: &mut dyn Read) -> InputReadResult {
    let mut result = InputReadResult {
        records: Vec::new(),
        errors: Vec::new(),
    };
    let mut next_id = 0;
    let mut stdin_used = false;
    let sources = if files.is_empty() {
        vec![None]
    } else {
        files.iter().map(Some).collect()
    };

    for source in sources {
        let (source, bytes) = match source {
            None => {
                stdin_used = true;
                (
                    Source::stdin(),
                    read_stream(stdin, "stdin", &mut result.errors),
                )
            }
            Some(path) if path.as_os_str() == OsStr::new("-") => {
                if stdin_used {
                    result.errors.push(InputError::RepeatedStdin);
                    continue;
                }
                stdin_used = true;
                (
                    Source::stdin(),
                    read_stream(stdin, "stdin", &mut result.errors),
                )
            }
            Some(path) => {
                let source = Source::file(path.clone());
                let bytes = match File::open(path).and_then(read_file) {
                    Ok(bytes) => Some(bytes),
                    Err(error) => {
                        result.errors.push(InputError::Read {
                            input_source: source.label(),
                            message: error.to_string(),
                        });
                        None
                    }
                };
                (source, bytes)
            }
        };

        if let Some(bytes) = bytes {
            match parse_bytes(&bytes, source, &mut next_id) {
                Ok(mut records) => result.records.append(&mut records),
                Err(error) => result.errors.push(error),
            }
        }
    }
    result
}

fn read_stream(
    stream: &mut dyn Read,
    source: &str,
    errors: &mut Vec<InputError>,
) -> Option<Vec<u8>> {
    match read_all(stream) {
        Ok(bytes) => Some(bytes),
        Err(error) => {
            errors.push(InputError::Read {
                input_source: source.to_owned(),
                message: error.to_string(),
            });
            None
        }
    }
}

fn read_file(file: File) -> io::Result<Vec<u8>> {
    let mut file = file;
    read_all(&mut file)
}

fn read_all(reader: &mut dyn Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn parse_bytes(
    bytes: &[u8],
    source: Source,
    next_id: &mut u64,
) -> Result<Vec<InputRecord>, InputError> {
    let mut records = Vec::new();
    let mut start = 0;
    let mut line_number = 1;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if byte == b'\n' {
            records.push(parse_line(
                &bytes[start..index],
                true,
                source.clone(),
                line_number,
                next_id,
            )?);
            start = index + 1;
            line_number += 1;
        }
    }
    if start < bytes.len() {
        records.push(parse_line(
            &bytes[start..],
            false,
            source,
            line_number,
            next_id,
        )?);
    }
    Ok(records)
}

fn parse_line(
    bytes: &[u8],
    terminated: bool,
    source: Source,
    line_number: usize,
    next_id: &mut u64,
) -> Result<InputRecord, InputError> {
    let (text_bytes, line_ending) = if terminated && bytes.last() == Some(&b'\r') {
        (&bytes[..bytes.len() - 1], LineEnding::CrLf)
    } else if terminated {
        (bytes, LineEnding::Lf)
    } else {
        (bytes, LineEnding::None)
    };
    let text = str::from_utf8(text_bytes).map_err(|error| InputError::InvalidUtf8 {
        input_source: source.label(),
        offset: error.valid_up_to(),
    })?;
    let id = CandidateId::new(*next_id);
    *next_id = next_id.checked_add(1).ok_or(InputError::TooManyRecords)?;
    Ok(InputRecord {
        candidate: Candidate::new(id, text),
        source,
        line_number,
        line_ending,
    })
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
