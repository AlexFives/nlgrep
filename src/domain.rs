use crate::error::DomainError;
use std::{collections::HashSet, ops::Range, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query(String);

impl Query {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainError::EmptyQuery);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CandidateId(u64);

impl CandidateId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: CandidateId,
    pub text: String,
}

impl Candidate {
    pub fn new(id: CandidateId, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Probability(f64);

impl Probability {
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Probability {
    type Error = DomainError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(DomainError::InvalidProbability(value.to_string()))
        }
    }
}

impl FromStr for Probability {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = value
            .parse::<f64>()
            .map_err(|_| DomainError::InvalidProbability(value.to_owned()))?;
        Self::try_from(parsed)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Judgment {
    pub id: CandidateId,
    pub probability: Probability,
}

impl Judgment {
    pub const fn new(id: CandidateId, probability: Probability) -> Self {
        Self { id, probability }
    }
}

#[derive(Debug)]
pub struct LogicalRequest<'a> {
    pub query: &'a Query,
    pub items: &'a [Candidate],
}

#[derive(Debug)]
pub struct BatchRequest<'a> {
    pub query: &'a Query,
    pub items: &'a [Candidate],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchPlan {
    pub ranges: Vec<Range<usize>>,
}

impl BatchPlan {
    pub fn new(ranges: Vec<Range<usize>>) -> Self {
        Self { ranges }
    }

    pub fn validate(&self, item_count: usize) -> Result<(), DomainError> {
        if item_count == 0 {
            return if self.ranges.is_empty() {
                Ok(())
            } else {
                Err(DomainError::InvalidBatchPlan(
                    "empty input must have no ranges".to_owned(),
                ))
            };
        }

        let mut next_start = 0;
        for range in &self.ranges {
            if range.start != next_start || range.start >= range.end || range.end > item_count {
                return Err(DomainError::InvalidBatchPlan(
                    "ranges must be non-empty, contiguous, and in bounds".to_owned(),
                ));
            }
            next_start = range.end;
        }

        if next_start != item_count {
            return Err(DomainError::InvalidBatchPlan(
                "ranges must cover every input item exactly once".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchResponse {
    pub judgments: Vec<Judgment>,
}

impl BatchResponse {
    pub fn try_new(
        expected_ids: &[CandidateId],
        mut judgments: Vec<Judgment>,
    ) -> Result<Self, DomainError> {
        let expected = expected_ids.iter().copied().collect::<HashSet<_>>();
        if expected.len() != expected_ids.len() {
            return Err(DomainError::InvalidBatchResponse(
                "expected candidate IDs contain duplicates".to_owned(),
            ));
        }
        if judgments.len() != expected_ids.len() {
            return Err(DomainError::InvalidBatchResponse(
                "response does not contain exactly one judgment per candidate".to_owned(),
            ));
        }

        let mut seen = HashSet::with_capacity(judgments.len());
        for judgment in &judgments {
            if !expected.contains(&judgment.id) {
                return Err(DomainError::InvalidBatchResponse(
                    "response contains an unknown candidate ID".to_owned(),
                ));
            }
            if !seen.insert(judgment.id) {
                return Err(DomainError::InvalidBatchResponse(
                    "response contains a duplicate candidate ID".to_owned(),
                ));
            }
        }

        judgments.sort_by_key(|judgment| judgment.id);
        Ok(Self { judgments })
    }
}

#[cfg(test)]
#[path = "domain_tests.rs"]
mod tests;
