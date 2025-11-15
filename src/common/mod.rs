// Common utilities and shared code

pub mod error;
pub mod config;
pub mod types;
pub mod utils;

pub use error::{Error, Result};
pub use config::{ClientConfig, ServerConfig};
pub use types::*;
