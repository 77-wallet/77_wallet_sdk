//! Multipart transport for backend-owned log storage, independent of ECDH requests.
use reqwest::{Client, Request, StatusCode, Url, multipart, redirect::Policy};
use serde::Deserialize;
use std::{fmt, time::Duration};

const MAX_ACK_BYTES: usize = 64 * 1024;

#[derive(Clone, Deserialize)]
pub struct LogUploadConfig {
    pub url: String,
    pub key: String,
    pub secret: String,
}

impl fmt::Debug for LogUploadConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogUploadConfig")
            .field("url", &"[configured endpoint]")
            .field("key", &"[redacted]")
            .field("secret", &"[redacted]")
            .finish()
    }
}

/// Errors deliberately exclude response bodies, response data, and credentials.
#[derive(Debug, thiserror::Error)]
pub enum LogUploadError {
    #[error("invalid log upload configuration")]
    InvalidConfig,
    #[error("invalid log upload metadata")]
    InvalidMetadata,
    #[error("failed to build log upload request")]
    Build,
    #[error("log upload transport failed")]
    Transport,
    #[error("log upload timed out")]
    Timeout,
    #[error("log upload returned HTTP {0}")]
    Http(u16),
    #[error("invalid log upload acknowledgement")]
    InvalidAcknowledgement,
    #[error("backend rejected log upload: code={code}, msg={message}")]
    Rejected { code: String, message: String },
}

#[derive(Debug)]
pub struct LogUploadClient {
    client: Client,
    config: LogUploadConfig,
    endpoint: Url,
}

impl LogUploadClient {
    pub fn new(config: LogUploadConfig) -> Result<Self, LogUploadError> {
        let endpoint = Url::parse(&config.url).map_err(|_| LogUploadError::InvalidConfig)?;
        if !matches!(endpoint.scheme(), "http" | "https")
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || config.key.trim().is_empty()
            || config.secret.trim().is_empty()
        {
            return Err(LogUploadError::InvalidConfig);
        }
        // A 307/308 redirect could otherwise forward authentication and log content.
        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|_| LogUploadError::Build)?;
        Ok(Self { client, config, endpoint })
    }

    fn build_request(
        &self,
        sn: &str,
        filename: &str,
        data: Vec<u8>,
    ) -> Result<Request, LogUploadError> {
        if sn.trim().is_empty()
            || filename.trim().is_empty()
            || matches!(filename, "." | "..")
            || filename.chars().any(|c| c.is_control() || c == '/' || c == '\\')
        {
            return Err(LogUploadError::InvalidMetadata);
        }
        let file = multipart::Part::bytes(data)
            .file_name(filename.to_owned())
            .mime_str("text/plain")
            .map_err(|_| LogUploadError::Build)?;
        let form = multipart::Form::new()
            .text("key", self.config.key.clone())
            .text("secret", self.config.secret.clone())
            .text("sn", sn.to_owned())
            .part("file", file);
        self.client
            .post(self.endpoint.clone())
            .multipart(form)
            .build()
            .map_err(|_| LogUploadError::Build)
    }

    /// Returns success only for HTTP 2xx with an explicit `success: true` envelope.
    /// No automatic retry: callers retain pending log data until acknowledgement.
    pub async fn upload(
        &self,
        sn: &str,
        filename: &str,
        data: Vec<u8>,
    ) -> Result<(), LogUploadError> {
        let request = self.build_request(sn, filename, data)?;
        let mut response = self.client.execute(request).await.map_err(transport_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(LogUploadError::Http(status.as_u16()));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            if chunk.len() > MAX_ACK_BYTES - body.len() {
                return Err(LogUploadError::InvalidAcknowledgement);
            }
            body.extend_from_slice(&chunk);
        }
        validate_acknowledgement(status, &body)
    }
}

fn transport_error(error: reqwest::Error) -> LogUploadError {
    if error.is_timeout() { LogUploadError::Timeout } else { LogUploadError::Transport }
}

fn validate_acknowledgement(status: StatusCode, body: &[u8]) -> Result<(), LogUploadError> {
    if !status.is_success() {
        return Err(LogUploadError::Http(status.as_u16()));
    }
    // Same success flag as BackendResponseOk. Upload needs no returned data, so it
    // neither decrypts `data` nor logs/propagates a potentially sensitive `msg`.
    #[derive(Deserialize)]
    struct Acknowledgement {
        success: bool,
        #[serde(default)]
        code: Option<serde_json::Value>,
        #[serde(default)]
        msg: Option<String>,
    }
    let ack: Acknowledgement =
        serde_json::from_slice(body).map_err(|_| LogUploadError::InvalidAcknowledgement)?;
    if ack.success {
        Ok(())
    } else {
        let code = ack.code.as_ref().map(format_ack_code).unwrap_or_else(|| "unknown".to_string());
        let message = ack
            .msg
            .as_deref()
            .map(|value| sanitize_ack_field(value, 512))
            .unwrap_or_else(|| "unknown".to_string());
        Err(LogUploadError::Rejected { code, message })
    }
}

fn format_ack_code(value: &serde_json::Value) -> String {
    let raw = match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        _ => "unknown".to_string(),
    };
    sanitize_ack_field(&raw, 128)
}

fn sanitize_ack_field(value: &str, max_chars: usize) -> String {
    let sanitized: String = value
        .chars()
        .take(max_chars)
        .map(|character| if character.is_control() { ' ' } else { character })
        .collect();
    if sanitized.trim().is_empty() { "unknown".to_string() } else { sanitized }
}

#[cfg(test)]
#[path = "log_upload_tests.rs"]
mod tests;
