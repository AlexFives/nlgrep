use crate::{BatchPlan, BatchRequest, BatchResponse, LogicalRequest, error::AdapterError};
use std::{future::Future, pin::Pin};

pub mod typesafe;

pub use typesafe::{TypeSafeAdapter, TypeSafeConfig};

pub fn build_adapter(config: TypeSafeConfig) -> Result<Box<dyn ModelAdapter>, crate::AdapterError> {
    TypeSafeAdapter::new(config).map(|adapter| Box::new(adapter) as Box<dyn ModelAdapter>)
}

pub type AdapterFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AdapterError>> + Send + 'a>>;

pub trait ModelAdapter: Send + Sync {
    fn plan_batches(&self, request: &LogicalRequest<'_>) -> Result<BatchPlan, AdapterError>;

    fn classify_batch<'a>(
        &'a self,
        request: &'a BatchRequest<'a>,
    ) -> AdapterFuture<'a, BatchResponse>;
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
