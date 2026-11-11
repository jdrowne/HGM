use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use clap::ValueEnum;
use regex::Regex;
use serde_json::{Value, json};

pub const MAX_OUTPUT_TOKENS: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Provider {
    #[value(name = "openai")]
    OpenAi,
    #[value(name = "vllm")]
    Vllm,
    #[value(name = "openrouter")]
    OpenRouter,
}

impl Provider {
    pub fn name(self) -> String {
        self.to_possible_value()
            .map(|v| v.get_name().to_owned())
            .expect("every provider has a name")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub provider: Provider,
    pub base_url: String,
    pub api_key_file: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Client {
    pub provider: Provider,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

#[derive(Debug)]
pub enum Error {
    Http(String),
    Status { code: u16, body: String },
    Response(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Http(e) => write!(f, "{e}"),
            Error::Status { code, body } => write!(f, "Error code: {code} - {body}"),
            Error::Response(e) => write!(f, "unexpected response: {e}"),
        }
    }
}

impl std::error::Error for Error {}

pub fn read_api_key(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map(|key| key.trim().to_owned())
        .map_err(|e| format!("{}: {e}", path.display()))
}

impl Service {
    pub fn client(&self, model: &str, api_key: &str) -> Client {
        Client {
            provider: self.provider,
            model: model.to_owned(),
            base_url: self.base_url.clone(),
            api_key: Some(api_key.to_owned()),
        }
    }
}

pub fn create_client(model: &str, service: &Service) -> Result<Client, String> {
    read_api_key(&service.api_key_file).map(|key| service.client(model, &key))
}

fn http() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(600)))
        .http_status_as_error(false)
        .build()
        .into()
}

impl Client {
    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base_url.trim_end_matches('/'))
    }

    fn read(mut response: ureq::http::Response<ureq::Body>) -> Result<Value, Error> {
        let code = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| Error::Http(e.to_string()))?;
        if !(200..300).contains(&code) {
            return Err(Error::Status { code, body });
        }
        serde_json::from_str(&body).map_err(|e| Error::Response(format!("{e}: {body}")))
    }

    pub fn post(&self, path: &str, body: &Value) -> Result<Value, Error> {
        let mut request = http().post(&self.url(path));
        if let Some(key) = &self.api_key {
            request = request.header("Authorization", &format!("Bearer {key}"));
        }
        let response = request
            .send_json(body)
            .map_err(|e| Error::Http(e.to_string()))?;
        Self::read(response)
    }

    pub fn get(&self, path: &str) -> Result<Value, Error> {
        let mut request = http().get(&self.url(path));
        if let Some(key) = &self.api_key {
            request = request.header("Authorization", &format!("Bearer {key}"));
        }
        let response = request.call().map_err(|e| Error::Http(e.to_string()))?;
        Self::read(response)
    }

    pub fn served_model(&self) -> Result<String, Error> {
        let models = self.get("models")?;
        models["data"][0]["id"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::Response(format!("no served model in {models}")))
    }

    pub fn chat(&self, body: &Value) -> Result<String, Error> {
        let response = self.post("chat/completions", body)?;
        response["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::Response(format!("no message content in {response}")))
    }
}

pub fn message(role: &str, content: &str) -> Value {
    json!({"role": role, "content": content})
}

pub fn get_response_from_llm(
    msg: &str,
    client: &Client,
    system_message: &str,
    msg_history: &[Value],
    temperature: f64,
) -> Result<(String, Vec<Value>), Error> {
    let model = client.model.as_str();
    let mut history = msg_history.to_vec();
    let content = if model.starts_with('o') {
        history.push(message("user", &format!("{system_message}{msg}")));
        client.chat(&json!({
            "model": model,
            "messages": history,
            "temperature": 1,
            "n": 1,
            "seed": 0,
        }))?
    } else {
        history.push(message("user", msg));
        let mut messages = vec![message("system", system_message)];
        messages.extend(history.iter().cloned());
        if model.to_lowercase().contains("gpt") {
            client.chat(&json!({
                "model": model,
                "messages": messages,
                "n": 1,
                "stop": null,
                "seed": 0,
            }))?
        } else {
            let model = match client.provider {
                Provider::Vllm => client.served_model()?,
                _ => model.to_owned(),
            };
            client.chat(&json!({
                "model": model,
                "messages": messages,
                "temperature": temperature,
                "max_tokens": MAX_OUTPUT_TOKENS,
                "n": 1,
                "stop": null,
            }))?
        }
    };
    history.push(message("assistant", &content));
    Ok((content, history))
}

static CONTROL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\x00-\x1F\x7F]").unwrap());

static OBJECT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)\{.*?\}").unwrap());

fn parse_or_clean(text: &str) -> Option<Value> {
    serde_json::from_str(text)
        .ok()
        .or_else(|| serde_json::from_str(&CONTROL.replace_all(text, "")).ok())
}

pub fn extract_json_between_markers(llm_output: &str) -> Option<Value> {
    let mut inside = false;
    let mut lines = Vec::new();
    for line in llm_output.split('\n') {
        let stripped = line.trim();
        if stripped.starts_with("```json") {
            inside = true;
            continue;
        }
        if inside && stripped.starts_with("```") {
            break;
        }
        if inside {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        return OBJECT
            .find_iter(llm_output)
            .map(|m| m.as_str().trim())
            .filter(|c| !c.is_empty())
            .find_map(parse_or_clean);
    }
    parse_or_clean(lines.join("\n").trim())
}
