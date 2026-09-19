#[path = "typesafe_batching.rs"]
mod batching;
#[path = "typesafe_wire.rs"]
mod wire;

use super::{AdapterFuture, ModelAdapter};
use crate::{AdapterError, BatchPlan, BatchRequest, BatchResponse, LogicalRequest, Probability};
use reqwest::{Client, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use std::{collections::BTreeMap, time::Duration};
use tokio::time::sleep;

const DEFAULT_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const DEFAULT_RETRIES: usize = 1;
const DEFAULT_BACKOFF: Duration = Duration::from_millis(100);

pub struct TypeSafeConfig {
    api_key: SecretString,
    endpoint: String,
    model: String,
    timeout: Duration,
    max_retries: usize,
    initial_backoff: Duration,
}

impl TypeSafeConfig {
    pub fn new(api_key: SecretString, model: String, timeout: Duration) -> Self {
        Self {
            api_key,
            endpoint: DEFAULT_ENDPOINT.to_owned(),
            model,
            timeout,
            max_retries: DEFAULT_RETRIES,
            initial_backoff: DEFAULT_BACKOFF,
        }
    }

    pub fn for_tests(endpoint: impl Into<String>, api_key: impl Into<String>) -> Self {
        let mut config = Self::new(
            SecretString::from(api_key.into()),
            "jev-latest".to_owned(),
            Duration::from_secs(30),
        );
        config.endpoint = format!("{}/v1/systemone", endpoint.into().trim_end_matches('/'));
        config.initial_backoff = Duration::ZERO;
        config
    }
}

pub struct TypeSafeAdapter {
    client: Client,
    config: TypeSafeConfig,
}

impl TypeSafeAdapter {
    pub fn new(config: TypeSafeConfig) -> Result<Self, AdapterError> {
        let client = Client::builder().timeout(config.timeout).build();
        Self::from_client_result(config, client)
    }

    fn from_client_result(
        config: TypeSafeConfig,
        client: Result<Client, reqwest::Error>,
    ) -> Result<Self, AdapterError> {
        let client = client.map_err(|error| AdapterError::Configuration(error.to_string()))?;
        Ok(Self { client, config })
    }

    pub fn for_tests(
        endpoint: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, AdapterError> {
        Self::new(TypeSafeConfig::for_tests(endpoint, api_key))
    }

    fn question(index: usize) -> wire::NoulQuestion {
        wire::NoulQuestion {
            kind: "noul",
            instructions: format!(
                "Does `items[{index}].text` satisfy the condition described by `query`?"
            ),
        }
    }

    fn request_body<'a>(&self, request: &BatchRequest<'a>) -> wire::RequestBody<'a> {
        let items = request
            .items
            .iter()
            .map(|candidate| wire::StateItem {
                text: &candidate.text,
            })
            .collect();
        let questions = request
            .items
            .iter()
            .enumerate()
            .map(|(index, _)| (format!("item_{index}"), Self::question(index)))
            .collect::<BTreeMap<_, _>>();
        wire::RequestBody {
            state: wire::RequestState {
                query: request.query.as_str(),
                items,
            },
            model: self.config.model.clone(),
            questions,
        }
    }

    async fn classify_batch_async(
        &self,
        request: &BatchRequest<'_>,
    ) -> Result<BatchResponse, AdapterError> {
        let body = self.request_body(request);
        let mut retry_count = 0;
        loop {
            let response = self
                .client
                .post(&self.config.endpoint)
                .bearer_auth(self.config.api_key.expose_secret())
                .json(&body)
                .send()
                .await;
            match response {
                Ok(response) if response.status().is_success() => {
                    return self.parse_response(response, request).await;
                }
                Ok(response) => {
                    let status = response.status();
                    if self.should_retry_status(status, retry_count) {
                        self.wait_before_retry(retry_count).await;
                        retry_count += 1;
                        continue;
                    }
                    return Err(self.status_error(status));
                }
                Err(error) => {
                    if self.should_retry_error(&error, retry_count) {
                        self.wait_before_retry(retry_count).await;
                        retry_count += 1;
                        continue;
                    }
                    return Err(if error.is_timeout() {
                        AdapterError::Timeout
                    } else {
                        AdapterError::Transport("HTTP request failed".to_owned())
                    });
                }
            }
        }
    }

    async fn parse_response(
        &self,
        response: reqwest::Response,
        request: &BatchRequest<'_>,
    ) -> Result<BatchResponse, AdapterError> {
        let body = response.json::<wire::ResponseBody>().await.map_err(|_| {
            AdapterError::InvalidResponse("response body is not valid TypeSafe JSON".to_owned())
        })?;
        let _usage = (body.usage.input_tokens, body.usage.output_tokens);
        let expected_ids = request.items.iter().map(|item| item.id).collect::<Vec<_>>();
        if body.answers.len() != expected_ids.len() {
            return Err(AdapterError::InvalidResponse(
                "answer count does not match request count".to_owned(),
            ));
        }
        let mut judgments = Vec::with_capacity(expected_ids.len());
        for (index, candidate_id) in expected_ids.iter().copied().enumerate() {
            let key = format!("item_{index}");
            let answer = body.answers.get(&key).ok_or_else(|| {
                AdapterError::InvalidResponse("response is missing a question ID".to_owned())
            })?;
            if answer.kind != "noul" {
                return Err(AdapterError::InvalidResponse(
                    "response contains a non-noul answer".to_owned(),
                ));
            }
            let value = answer.noul.ok_or_else(|| {
                AdapterError::InvalidResponse("noul answer has no probability".to_owned())
            })?;
            let probability = Probability::try_from(value).map_err(|_| {
                AdapterError::InvalidResponse("noul probability is outside [0, 1]".to_owned())
            })?;
            judgments.push(crate::Judgment::new(candidate_id, probability));
        }
        BatchResponse::try_new(&expected_ids, judgments)
            .map_err(|error| AdapterError::InvalidResponse(error.to_string()))
    }

    fn should_retry_status(&self, status: StatusCode, retry_count: usize) -> bool {
        retry_count < self.config.max_retries
            && (status == StatusCode::TOO_MANY_REQUESTS || status.as_u16() == 529)
    }

    fn should_retry_error(&self, error: &reqwest::Error, retry_count: usize) -> bool {
        retry_count < self.config.max_retries && (error.is_connect() || error.is_timeout())
    }

    async fn wait_before_retry(&self, retry_count: usize) {
        let multiplier = 2_u32.saturating_pow(retry_count as u32);
        sleep(self.config.initial_backoff.saturating_mul(multiplier)).await;
    }

    fn status_error(&self, status: StatusCode) -> AdapterError {
        match status {
            StatusCode::UNAUTHORIZED => AdapterError::Authentication,
            status => AdapterError::ProviderRejected {
                status: status.as_u16(),
                message: "provider rejected the request".to_owned(),
            },
        }
    }

    fn empty_request_body<'a>(&self, query: &'a str) -> wire::RequestBody<'a> {
        wire::RequestBody {
            state: wire::RequestState {
                query,
                items: Vec::new(),
            },
            model: self.config.model.clone(),
            questions: BTreeMap::new(),
        }
    }
}

impl ModelAdapter for TypeSafeAdapter {
    fn plan_batches(&self, request: &LogicalRequest<'_>) -> Result<BatchPlan, AdapterError> {
        batching::plan(self, request)
    }

    fn classify_batch<'a>(
        &'a self,
        request: &'a BatchRequest<'a>,
    ) -> AdapterFuture<'a, BatchResponse> {
        Box::pin(async move { self.classify_batch_async(request).await })
    }
}

#[cfg(test)]
#[path = "typesafe_tests.rs"]
mod tests;
