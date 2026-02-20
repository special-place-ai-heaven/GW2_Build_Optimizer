//! Gemini API client for LLM-powered build reasoning.
//! Uses Google AI Studio's REST API (generativelanguage.googleapis.com).
//! API key is sent via x-goog-api-key header (not URL query) for security.

use serde::{Deserialize, Serialize};

const GEMINI_GENERATE_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent";
const GEMINI_MODELS_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models";

#[derive(Debug, thiserror::Error)]
pub enum GeminiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("Invalid API key")]
    InvalidKey,
    #[error("Rate limited")]
    RateLimited,
    #[error("Parse error: {0}")]
    Parse(String),
}

pub struct GeminiClient {
    api_key: String,
    http: reqwest::blocking::Client,
}

#[derive(Serialize)]
struct GenerateRequest {
    contents: Vec<Content>,
}

#[derive(Serialize, Deserialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Deserialize)]
struct GenerateResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<Content>,
}

impl GeminiClient {
    pub fn new(api_key: &str) -> Result<Self, GeminiError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Self {
            api_key: api_key.to_string(),
            http,
        })
    }

    /// Validate the API key using the models list endpoint (no quota consumed).
    pub fn validate_key(&self) -> Result<(), GeminiError> {
        let resp = self
            .http
            .get(GEMINI_MODELS_URL)
            .header("x-goog-api-key", &self.api_key)
            .send()?;

        match resp.status().as_u16() {
            200 => Ok(()),
            401 | 403 => Err(GeminiError::InvalidKey),
            429 => Err(GeminiError::RateLimited),
            status => {
                let body = resp.text().unwrap_or_default();
                Err(GeminiError::Api {
                    status,
                    message: body,
                })
            }
        }
    }

    /// Send a prompt to Gemini and return the response text.
    pub fn generate(&self, prompt: &str) -> Result<String, GeminiError> {
        let request = GenerateRequest {
            contents: vec![Content {
                parts: vec![Part {
                    text: prompt.to_string(),
                }],
            }],
        };

        let resp = self
            .http
            .post(GEMINI_GENERATE_URL)
            .header("x-goog-api-key", &self.api_key)
            .json(&request)
            .send()?;

        let status = resp.status().as_u16();
        match status {
            200 => {}
            401 | 403 => return Err(GeminiError::InvalidKey),
            429 => return Err(GeminiError::RateLimited),
            _ => {
                let body = resp.text().unwrap_or_default();
                return Err(GeminiError::Api {
                    status,
                    message: body,
                });
            }
        }

        let body: GenerateResponse = resp.json()?;
        let text = body
            .candidates
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.content)
            .and_then(|c| c.parts.into_iter().next())
            .map(|p| p.text)
            .ok_or_else(|| GeminiError::Parse("No response text from Gemini".into()))?;

        Ok(text)
    }
}
