//! Local semantic-model transports.
//!
//! The domain core owns prompting, validation and merge semantics. This module
//! only talks to explicitly configured loopback services (Ollama or an
//! OpenAI-compatible llama.cpp server). Documents are never sent to a remote
//! host by this transport.

use dokkomplekt_core::SemanticModel;
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read as _;
use std::net::IpAddr;
use std::time::Duration;

const MAX_MODEL_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_MODEL_PROMPT_CHARS: usize = 2_000_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LocalSemanticModelConfig {
    pub enabled: bool,
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    /// BCP-47-like source language tag or `auto`. Values are never translated.
    pub preferred_language: String,
    pub timeout_seconds: u64,
    /// Runs the model beside the deterministic parser and records only aggregate
    /// agreement metrics. Shadow results never change generated documents.
    pub shadow_mode: bool,
    /// Explicit opt-in for writing privacy-preserving ground-truth corpus entries
    /// after the specialist's final accepted case and generated kit are known.
    pub corpus_recording_enabled: bool,
    pub auto_apply_zero_touch: bool,
    pub consistency_passes: u8,
}

impl Default for LocalSemanticModelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "ollama".into(),
            endpoint: "http://127.0.0.1:11434".into(),
            model: "qwen2.5:7b-instruct".into(),
            preferred_language: "auto".into(),
            timeout_seconds: 90,
            shadow_mode: true,
            corpus_recording_enabled: false,
            auto_apply_zero_touch: false,
            consistency_passes: 2,
        }
    }
}

impl LocalSemanticModelConfig {
    pub fn validate(&self) -> Result<ValidatedLocalModelConfig, String> {
        let provider = match self.provider.trim().to_ascii_lowercase().as_str() {
            "ollama" => LocalModelProvider::Ollama,
            "llama_cpp" | "llama.cpp" | "openai_compatible" => LocalModelProvider::LlamaCpp,
            _ => return Err("Поддерживаются только провайдеры ollama и llama_cpp.".into()),
        };
        let mut endpoint = reqwest::Url::parse(self.endpoint.trim())
            .map_err(|error| format!("Некорректный адрес локальной модели: {error}"))?;
        if !matches!(endpoint.scheme(), "http" | "https") {
            return Err("Локальная модель должна использовать http:// или https://.".into());
        }
        if !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(
                "Адрес локальной модели не должен содержать credentials, query или fragment."
                    .into(),
            );
        }
        let host = endpoint
            .host_str()
            .ok_or_else(|| "В адресе локальной модели отсутствует host.".to_string())?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if !is_loopback_host(&host) {
            return Err(
                "SemanticModel разрешён только на localhost/127.0.0.1/::1: документы не должны уходить во внешнюю сеть."
                    .into(),
            );
        }
        if !endpoint.path().is_empty() && endpoint.path() != "/" {
            return Err("Укажите базовый адрес сервера без /api или /v1 в конце.".into());
        }
        endpoint.set_path("/");
        let model = self.model.trim().to_string();
        if model.is_empty() || model.len() > 200 || model.chars().any(char::is_control) {
            return Err("Укажите корректное имя локальной модели.".into());
        }
        let language = self.preferred_language.trim();
        let valid_language = language.eq_ignore_ascii_case("auto")
            || (!language.is_empty()
                && language.len() <= 32
                && language
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-'));
        if !valid_language {
            return Err(
                "Язык SemanticModel должен быть auto или BCP-47 тегом, например ru-RU/en-US."
                    .into(),
            );
        }
        if !(5..=600).contains(&self.timeout_seconds) {
            return Err("Тайм-аут SemanticModel должен быть от 5 до 600 секунд.".into());
        }
        if !(2..=3).contains(&self.consistency_passes) {
            return Err(
                "Число self-consistency проходов SemanticModel должно быть от 2 до 3.".into(),
            );
        }
        Ok(ValidatedLocalModelConfig {
            provider,
            endpoint,
            model,
            timeout_seconds: self.timeout_seconds,
            consistency_passes: self.consistency_passes,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalModelProvider {
    Ollama,
    LlamaCpp,
}

#[derive(Debug, Clone)]
pub struct ValidatedLocalModelConfig {
    provider: LocalModelProvider,
    endpoint: reqwest::Url,
    model: String,
    timeout_seconds: u64,
    consistency_passes: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalSemanticModelStatus {
    pub configured: bool,
    pub reachable: bool,
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    pub available_models: Vec<String>,
    pub message: String,
}

pub struct LocalSemanticModelTransport {
    config: ValidatedLocalModelConfig,
    client: Client,
}

impl LocalSemanticModelTransport {
    pub fn new(config: &LocalSemanticModelConfig) -> Result<Self, String> {
        let config = config.validate()?;
        crate::ensure_rustls_crypto_provider();
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|error| format!("Не удалось создать HTTP-клиент SemanticModel: {error}"))?;
        Ok(Self { config, client })
    }

    pub fn complete_many(&self, prompt: &str) -> Result<Vec<String>, String> {
        let mut outputs = Vec::with_capacity(usize::from(self.config.consistency_passes));
        for pass_index in 0..self.config.consistency_passes {
            outputs.push(self.complete_pass(prompt, pass_index)?);
        }
        Ok(outputs)
    }

    fn complete_pass(&self, prompt: &str, pass_index: u8) -> Result<String, String> {
        if prompt.chars().count() > MAX_MODEL_PROMPT_CHARS {
            return Err("Документ слишком велик для одного запроса SemanticModel.".into());
        }
        let profiled_prompt = consensus_prompt_variant(prompt, pass_index);
        let profile = consensus_sampling_profile(prompt, pass_index);
        match self.config.provider {
            LocalModelProvider::Ollama => self.complete_ollama(&profiled_prompt, profile),
            LocalModelProvider::LlamaCpp => self.complete_llama_cpp(&profiled_prompt, profile),
        }
    }

    pub fn status(&self) -> LocalSemanticModelStatus {
        match self.status_inner() {
            Ok(models) => LocalSemanticModelStatus {
                configured: true,
                reachable: true,
                provider: self.provider_name().into(),
                endpoint: self.config.endpoint.to_string(),
                model: self.config.model.clone(),
                available_models: models,
                message: "Локальная SemanticModel доступна; данные остаются на компьютере.".into(),
            },
            Err(error) => LocalSemanticModelStatus {
                configured: true,
                reachable: false,
                provider: self.provider_name().into(),
                endpoint: self.config.endpoint.to_string(),
                model: self.config.model.clone(),
                available_models: Vec::new(),
                message: error,
            },
        }
    }

    fn provider_name(&self) -> &'static str {
        match self.config.provider {
            LocalModelProvider::Ollama => "ollama",
            LocalModelProvider::LlamaCpp => "llama_cpp",
        }
    }

    fn status_inner(&self) -> Result<Vec<String>, String> {
        match self.config.provider {
            LocalModelProvider::Ollama => {
                let url = self.endpoint("api/tags")?;
                let response = self
                    .client
                    .get(url)
                    .send()
                    .map_err(|error| format!("Ollama недоступна: {error}"))?;
                let payload: OllamaTagsResponse = read_json_limited(response)?;
                Ok(payload
                    .models
                    .into_iter()
                    .filter_map(|model| {
                        let name = model.name.or(model.model)?.trim().to_string();
                        (!name.is_empty()).then_some(name)
                    })
                    .collect())
            }
            LocalModelProvider::LlamaCpp => {
                let url = self.endpoint("v1/models")?;
                let response = self
                    .client
                    .get(url)
                    .send()
                    .map_err(|error| format!("llama.cpp server недоступен: {error}"))?;
                let payload: OpenAiModelsResponse = read_json_limited(response)?;
                Ok(payload
                    .data
                    .into_iter()
                    .map(|model| model.id.trim().to_string())
                    .filter(|name| !name.is_empty())
                    .collect())
            }
        }
    }

    fn endpoint(&self, relative: &str) -> Result<reqwest::Url, String> {
        self.config
            .endpoint
            .join(relative)
            .map_err(|error| format!("Не удалось построить endpoint SemanticModel: {error}"))
    }

    fn complete_ollama(
        &self,
        prompt: &str,
        profile: ConsensusSamplingProfile,
    ) -> Result<String, String> {
        let url = self.endpoint("api/generate")?;
        let response = self
            .client
            .post(url)
            .json(&serde_json::json!({
                "model": self.config.model,
                "prompt": prompt,
                "stream": false,
                "format": "json",
                "options": {
                    "temperature": profile.temperature,
                    "seed": profile.seed
                }
            }))
            .send()
            .map_err(|error| format!("Ошибка запроса к Ollama: {error}"))?;
        let payload: OllamaGenerateResponse = read_json_limited(response)?;
        let text = payload.response.trim().to_string();
        if text.is_empty() {
            Err("Ollama вернула пустой ответ.".into())
        } else {
            Ok(text)
        }
    }

    fn complete_llama_cpp(
        &self,
        prompt: &str,
        profile: ConsensusSamplingProfile,
    ) -> Result<String, String> {
        let url = self.endpoint("v1/chat/completions")?;
        let response = self
            .client
            .post(url)
            .json(&serde_json::json!({
                "model": self.config.model,
                "messages": [
                    {"role": "system", "content": "Return only strict JSON. Never invent document facts."},
                    {"role": "user", "content": prompt}
                ],
                "temperature": profile.temperature,
                "seed": profile.seed,
                "stream": false
            }))
            .send()
            .map_err(|error| format!("Ошибка запроса к llama.cpp server: {error}"))?;
        let payload: OpenAiChatResponse = read_json_limited(response)?;
        let text = payload
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content.trim().to_string())
            .filter(|text| !text.is_empty())
            .ok_or_else(|| "llama.cpp server вернул ответ без текста.".to_string())?;
        Ok(text)
    }
}

impl SemanticModel for LocalSemanticModelTransport {
    fn complete(&self, prompt: &str) -> Result<String, String> {
        if prompt.chars().count() > MAX_MODEL_PROMPT_CHARS {
            return Err("Документ слишком велик для одного запроса SemanticModel.".into());
        }
        let profile = consensus_sampling_profile(prompt, 0);
        match self.config.provider {
            LocalModelProvider::Ollama => self.complete_ollama(prompt, profile),
            LocalModelProvider::LlamaCpp => self.complete_llama_cpp(prompt, profile),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ConsensusSamplingProfile {
    temperature: f32,
    seed: u64,
}

fn consensus_sampling_profile(prompt: &str, pass_index: u8) -> ConsensusSamplingProfile {
    let mut digest = Sha256::new();
    digest.update(b"dokkomplekt-semantic-consensus-v2\0");
    digest.update(prompt.as_bytes());
    digest.update([pass_index]);
    let bytes = digest.finalize();
    let mut seed_bytes = [0_u8; 8];
    seed_bytes.copy_from_slice(&bytes[..8]);
    let temperature = match pass_index {
        0 => 0.0,
        1 => 0.15,
        _ => 0.25,
    };
    ConsensusSamplingProfile {
        temperature,
        seed: u64::from_le_bytes(seed_bytes),
    }
}

fn consensus_prompt_variant(prompt: &str, pass_index: u8) -> String {
    let instruction = match pass_index {
        0 => "PASS A — independently extract only directly evidenced facts.",
        1 => "PASS B — independently verify the document from scratch; challenge names, dates, addresses and amounts and omit anything ambiguous.",
        _ => "PASS C — adversarial contradiction audit; prefer omitting a field over repeating an unsupported value.",
    };
    format!("{instruction}\nDo not rely on any previous pass.\n\n{prompt}")
}

fn is_loopback_host(host: &str) -> bool {
    if matches!(host, "localhost" | "localhost.localdomain") || host.ends_with(".localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

fn read_json_limited<T: for<'de> Deserialize<'de>>(response: Response) -> Result<T, String> {
    if !response.status().is_success() {
        return Err(format!("SemanticModel вернула HTTP {}.", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MODEL_RESPONSE_BYTES)
    {
        return Err("Ответ SemanticModel превышает 4 МБ.".into());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_MODEL_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Не удалось прочитать ответ SemanticModel: {error}"))?;
    if bytes.len() as u64 > MAX_MODEL_RESPONSE_BYTES {
        return Err("Ответ SemanticModel превышает 4 МБ.".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        format!("SemanticModel вернула некорректный JSON transport-ответ: {error}")
    })
}

#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaModelInfo>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelInfo {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    #[serde(default)]
    data: Vec<OpenAiModelInfo>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelInfo {
    id: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_endpoints_are_accepted() {
        let mut config = LocalSemanticModelConfig::default();
        assert!(config.validate().is_ok());
        config.endpoint = "http://localhost:11434".into();
        assert!(config.validate().is_ok());
        config.endpoint = "http://[::1]:8080".into();
        assert!(config.validate().is_ok());
        config.endpoint = "https://example.com".into();
        assert!(config.validate().is_err());
        config.endpoint = "http://192.168.1.20:11434".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn consensus_profiles_use_distinct_seeds_and_sampling() {
        let prompt = "document";
        let first = consensus_sampling_profile(prompt, 0);
        let second = consensus_sampling_profile(prompt, 1);
        let third = consensus_sampling_profile(prompt, 2);
        assert_ne!(first.seed, second.seed);
        assert_ne!(second.seed, third.seed);
        assert!(first.temperature < second.temperature);
        assert!(second.temperature < third.temperature);
        assert_ne!(
            consensus_prompt_variant(prompt, 0),
            consensus_prompt_variant(prompt, 1)
        );
    }

    #[test]
    fn provider_and_timeout_are_validated() {
        let mut config = LocalSemanticModelConfig {
            provider: "unknown".into(),
            ..LocalSemanticModelConfig::default()
        };
        assert!(config.validate().is_err());
        config.provider = "llama_cpp".into();
        config.endpoint = "http://127.0.0.1:8080".into();
        config.timeout_seconds = 601;
        assert!(config.validate().is_err());
        config.timeout_seconds = 30;
        assert!(config.validate().is_ok());
    }
}
