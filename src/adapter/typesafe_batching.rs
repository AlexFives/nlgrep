use super::{TypeSafeAdapter, wire};
use crate::{AdapterError, BatchPlan, LogicalRequest};
use serde::Serialize;

// Jev documents a 64k request limit and a 32k state-plus-longest-question limit.
const CONTEXT_TOKEN_LIMIT: usize = 64_000;
const STATE_QUESTION_TOKEN_LIMIT: usize = 32_000;
// Keep 10% of each documented limit for tokenizer and serialization variance.
const TOKEN_BUDGET_PERCENT: usize = 90;
// The API tokenizer is not available locally; three UTF-8 JSON bytes per token
// is a conservative estimate for the ASCII-heavy request shape we send.
const ESTIMATED_BYTES_PER_TOKEN: usize = 3;
const NON_ASCII_ESTIMATED_BYTES_PER_TOKEN: usize = 2;
const CONTEXT_TOKEN_BUDGET: usize = CONTEXT_TOKEN_LIMIT * TOKEN_BUDGET_PERCENT / 100;
const STATE_QUESTION_TOKEN_BUDGET: usize = STATE_QUESTION_TOKEN_LIMIT * TOKEN_BUDGET_PERCENT / 100;

#[derive(Clone, Copy)]
struct BatchSize {
    total_bytes: usize,
    state_bytes: usize,
    longest_question_bytes: usize,
    item_count: usize,
    contains_non_ascii: bool,
}

impl BatchSize {
    fn empty(total_bytes: usize, state_bytes: usize, contains_non_ascii: bool) -> Self {
        Self {
            total_bytes,
            state_bytes,
            longest_question_bytes: 0,
            item_count: 0,
            contains_non_ascii,
        }
    }

    fn with_item(
        self,
        state_item_bytes: usize,
        question_entry_bytes: usize,
        question_bytes: usize,
        item_contains_non_ascii: bool,
    ) -> Self {
        let separator_bytes = usize::from(self.item_count > 0);
        Self {
            total_bytes: self
                .total_bytes
                .saturating_add(state_item_bytes)
                .saturating_add(question_entry_bytes)
                .saturating_add(separator_bytes.saturating_mul(2)),
            state_bytes: self
                .state_bytes
                .saturating_add(state_item_bytes)
                .saturating_add(separator_bytes),
            longest_question_bytes: self.longest_question_bytes.max(question_bytes),
            item_count: self.item_count.saturating_add(1),
            contains_non_ascii: self.contains_non_ascii || item_contains_non_ascii,
        }
    }

    fn fits(self) -> bool {
        estimated_tokens(self.total_bytes, self.contains_non_ascii) <= CONTEXT_TOKEN_BUDGET
            && estimated_tokens(
                self.state_bytes.saturating_add(self.longest_question_bytes),
                self.contains_non_ascii,
            ) <= STATE_QUESTION_TOKEN_BUDGET
    }
}

fn estimated_tokens(bytes: usize, contains_non_ascii: bool) -> usize {
    let bytes_per_token = if contains_non_ascii {
        NON_ASCII_ESTIMATED_BYTES_PER_TOKEN
    } else {
        ESTIMATED_BYTES_PER_TOKEN
    };
    bytes.saturating_add(bytes_per_token - 1) / bytes_per_token
}

fn contains_non_ascii(text: &str) -> bool {
    text.bytes().any(|byte| byte >= 0x80)
}

fn serialized_size<T: Serialize>(value: &T) -> Result<usize, AdapterError> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|error| {
            AdapterError::Configuration(format!(
                "could not estimate TypeSafe request size: {error}"
            ))
        })
}

pub(super) fn plan(
    adapter: &TypeSafeAdapter,
    request: &LogicalRequest<'_>,
) -> Result<BatchPlan, AdapterError> {
    let empty_body = adapter.empty_request_body(request.query.as_str());
    let base_total_bytes = serialized_size(&empty_body)?;
    let base_state_bytes = serialized_size(&empty_body.state)?;
    let query_contains_non_ascii = contains_non_ascii(request.query.as_str());
    let mut ranges = Vec::new();
    let mut start = 0;

    while start < request.items.len() {
        let mut size =
            BatchSize::empty(base_total_bytes, base_state_bytes, query_contains_non_ascii);
        let mut end = start;
        while end < request.items.len() {
            let local_index = end - start;
            let state_item = wire::StateItem {
                text: &request.items[end].text,
            };
            let state_item_bytes = serialized_size(&state_item)?;
            let question = TypeSafeAdapter::question(local_index);
            let question_bytes = serialized_size(&question)?;
            let key_bytes = serialized_size(&format!("item_{local_index}"))?;
            let question_entry_bytes = key_bytes.saturating_add(1).saturating_add(question_bytes);
            let next_size = size.with_item(
                state_item_bytes,
                question_entry_bytes,
                question_bytes,
                contains_non_ascii(&request.items[end].text),
            );

            if end > start && !next_size.fits() {
                break;
            }
            size = next_size;
            end += 1;
        }
        ranges.push(start..end);
        start = end;
    }

    Ok(BatchPlan::new(ranges))
}
