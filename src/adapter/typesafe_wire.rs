use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize)]
pub(super) struct RequestBody<'a> {
    pub state: RequestState<'a>,
    pub model: String,
    pub questions: BTreeMap<String, NoulQuestion>,
}

#[derive(Serialize)]
pub(super) struct RequestState<'a> {
    pub query: &'a str,
    pub items: Vec<StateItem<'a>>,
}

#[derive(Serialize)]
pub(super) struct StateItem<'a> {
    pub text: &'a str,
}

#[derive(Serialize)]
pub(super) struct NoulQuestion {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub instructions: String,
}

#[derive(Deserialize)]
pub(super) struct ResponseBody {
    pub answers: BTreeMap<String, Answer>,
    pub usage: Usage,
}

#[derive(Deserialize)]
pub(super) struct Answer {
    #[serde(rename = "type")]
    pub kind: String,
    pub noul: Option<f64>,
}

#[derive(Deserialize)]
pub(super) struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}
