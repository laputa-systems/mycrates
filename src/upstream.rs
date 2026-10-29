use crate::error::{Error, Result};
use std::path::Path;

#[derive(Clone)]
pub struct UpstreamClient {
    pub index_url: String,
}

impl UpstreamClient {
    pub fn new(index_url: String) -> Self {
        Self { index_url }
    }
}
