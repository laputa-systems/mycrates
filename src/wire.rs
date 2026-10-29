use crate::error::{Error, Result};
use crate::model::PublishBody;
use std::io::Read;

#[derive(Debug)]
pub struct DecodedPublishRequest {
    pub body: PublishBody,
    pub crate_bytes: Vec<u8>,
}

pub fn decode_publish_payload<R: Read>(
    mut reader: R,
    content_length: u64,
    max_compressed: u64,
) -> Result<DecodedPublishRequest> {
    if content_length < 8 {
        return Err(Error::Input("upload payload too short".to_string()));
    }
    if content_length > max_compressed + 10 * 1024 * 1024 {
        return Err(Error::Limit("upload payload exceeds size limit".to_string()));
    }

    let mut json_len_buf = [0u8; 4];
    reader
        .read_exact(&mut json_len_buf)
        .map_err(|e| Error::Input(format!("failed to read json length: {e}")))?;
    let json_len = u32::from_le_bytes(json_len_buf) as u64;

    if json_len == 0 {
        return Err(Error::Input("empty json metadata".to_string()));
    }
    if json_len > 10 * 1024 * 1024 || json_len > content_length - 8 {
        return Err(Error::Limit("json metadata length invalid or too large".to_string()));
    }

    let mut json_buf = vec![0u8; json_len as usize];
    reader
        .read_exact(&mut json_buf)
        .map_err(|e| Error::Input(format!("failed to read json body: {e}")))?;

    let body: PublishBody = serde_json::from_slice(&json_buf)
        .map_err(|e| Error::Input(format!("invalid publish json: {e}")))?;

    let mut crate_len_buf = [0u8; 4];
    reader
        .read_exact(&mut crate_len_buf)
        .map_err(|e| Error::Input(format!("failed to read crate length: {e}")))?;
    let crate_len = u32::from_le_bytes(crate_len_buf) as u64;

    if crate_len == 0 {
        return Err(Error::Input("empty crate archive".to_string()));
    }
    if crate_len > max_compressed {
        return Err(Error::Limit("crate archive exceeds max compressed size".to_string()));
    }
    if 4 + json_len + 4 + crate_len != content_length {
        return Err(Error::Input("crate length does not match content length framing".to_string()));
    }

    let mut crate_bytes = vec![0u8; crate_len as usize];
    reader
        .read_exact(&mut crate_bytes)
        .map_err(|e| Error::Input(format!("failed to read crate bytes: {e}")))?;

    Ok(DecodedPublishRequest { body, crate_bytes })
}
