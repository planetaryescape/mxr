pub mod auth;
pub mod error;
pub mod smtp;
mod token_cache_file;

pub use auth::{OutlookAuth, OutlookTenant, OutlookTokens, BUNDLED_CLIENT_ID};
pub use error::OutlookError;
pub use smtp::OutlookSmtpSendProvider;
