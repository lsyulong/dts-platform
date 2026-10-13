mod ast;
mod expression;
mod grants;
mod input;
mod masking_lineage;
mod metrics;
mod quality;
mod query;
mod recognition;
mod request;
mod schemas;

pub use dts_backend::repo::data_access::AccessOutcome;
pub use grants::EffectiveDataGrant;
pub(crate) use masking_lineage::{masking_outputs, masking_table};
pub use query::{AuthorizedQuery, QueryAuthorizationService};
pub use request::DataAccessService;

use crate::models::error::AppError;

fn bad(message: impl Into<String>) -> AppError {
    AppError::BadRequest(message.into())
}
fn denied(message: impl Into<String>) -> AppError {
    AppError::Forbidden(message.into())
}

#[cfg(test)]
mod tests;
