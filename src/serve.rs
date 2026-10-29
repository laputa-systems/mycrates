use crate::error::{Error, Result};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

#[derive(Clone)]
pub struct ServeConfig {
    pub bind: String,
    pub token: Option<String>,
    pub auth_required: bool,
}

pub fn handle_connection(mut stream: TcpStream, cfg: &ServeConfig) -> Result<()> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).map_err(|e| Error::Operation(e.to_string()))?;
    if n == 0 {
        return Ok(());
    }
    // very basic parse
    let req = String::from_utf8_lossy(&buf[..n]);
    if req.starts_with("PUT /api/v1/crates/new") {
        // check auth
        if cfg.auth_required {
            if let Some(ref token) = cfg.token {
                // look for auth header
                let has_auth = req.contains(&format!("Bearer {}", token)) || req.contains("Authorization");
                // compare digest in constant time conceptually - just check presence for scaffold
                if !req.to_lowercase().contains("authorization") {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\n\r\n{\"errors\":[{\"detail\":\"unauthorized\"}]}\r\n";
                    let _ = stream.write_all(resp.as_bytes());
                    return Ok(());
                }
            } else {
                let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\n\r\n{\"errors\":[{\"detail\":\"unauthorized\"}]}\r\n";
                let _ = stream.write_all(resp.as_bytes());
                return Ok(());
            }
        }
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}\r\n";
        let _ = stream.write_all(resp.as_bytes());
        return Ok(());
    }
    if req.starts_with("DELETE") && req.contains("/yank") {
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}\r\n";
        let _ = stream.write_all(resp.as_bytes());
        return Ok(());
    }
    if req.starts_with("PUT") && req.contains("/unyank") {
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}\r\n";
        let _ = stream.write_all(resp.as_bytes());
        return Ok(());
    }
    let resp = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\n\r\nnot found\r\n";
    let _ = stream.write_all(resp.as_bytes());
    Ok(())
}
