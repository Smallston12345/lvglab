//! Client for the Google Gemini API (`streamGenerateContent` with
//! server-sent events). The answer's JSON schema is enforced with
//! `generationConfig.responseFormat`.

use std::io::{BufRead, BufReader};
use std::time::Duration;

use serde_json::{Value, json};

use crate::client::{AiError, Answer, Config, Usage};
use crate::spec::{Design, schema};

const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const MAX_TOKENS: u32 = 64000;

/// Model ID without an optional `models/` prefix.
fn model_id(model: &str) -> &str {
    let model = model.trim();
    model.strip_prefix("models/").unwrap_or(model)
}

/// Streaming endpoint of a model.
pub fn api_url(model: &str) -> String {
    format!("{API_BASE}/{}:streamGenerateContent?alt=sse", model_id(model))
}

/// Gemini thinking level for an effort. The Gemini 4 models support LOW,
/// MEDIUM and HIGH; the editor's two highest levels use HIGH.
fn thinking_level(effort: &str) -> &'static str {
    match effort {
        "low" => "LOW",
        "medium" => "MEDIUM",
        _ => "HIGH",
    }
}

/// Builds the request body. Kept separate for tests.
pub fn request_body(config: &Config, system: &str, user: &str) -> Value {
    json!({
        "systemInstruction": { "parts": [{ "text": system }] },
        "contents": [{ "role": "user", "parts": [{ "text": user }] }],
        "generationConfig": {
            "maxOutputTokens": MAX_TOKENS,
            "responseFormat": { "text": { "mimeType": "APPLICATION_JSON", "schema": schema() } },
            "thinkingConfig": { "thinkingLevel": thinking_level(&config.effort) }
        }
    })
}

/// Sends the request and returns the parsed design.
pub fn generate(
    config: &Config,
    system: &str,
    user: &str,
    mut on_progress: impl FnMut(usize),
) -> Result<Answer, AiError> {
    if config.api_key.trim().is_empty() {
        return Err(AiError::MissingKey);
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(300))
        .build();
    let request = agent
        .post(&api_url(&config.model))
        .set("content-type", "application/json")
        .set("x-goog-api-key", config.api_key.trim());
    let response = match request.send_json(request_body(config, system, user)) {
        Ok(response) => response,
        Err(ureq::Error::Status(status, response)) => return Err(status_error(status, response)),
        Err(err) => return Err(AiError::Network(err.to_string())),
    };
    let stream = read_stream(BufReader::new(response.into_reader()), &mut on_progress)?;
    if stream.blocked {
        return Err(AiError::Refused);
    }
    match stream.finish_reason.as_deref() {
        Some("MAX_TOKENS") => return Err(AiError::Truncated),
        Some("SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" | "LANGUAGE") => {
            return Err(AiError::Refused);
        }
        _ => {}
    }
    let text = stream.text.trim();
    if text.is_empty() {
        let reason = stream.finish_reason.unwrap_or_default();
        return Err(AiError::InvalidAnswer(format!("empty answer {reason}").trim().to_owned()));
    }
    let design = serde_json::from_str::<Design>(text).map_err(|e| AiError::InvalidAnswer(e.to_string()))?;
    let model = if stream.model.is_empty() { model_id(&config.model).to_owned() } else { stream.model };
    Ok(Answer { design, usage: stream.usage, model })
}

fn status_error(status: u16, response: ureq::Response) -> AiError {
    let error = response.into_json::<Value>().ok().map(|v| v["error"].clone()).unwrap_or_default();
    let message = error["message"].as_str().unwrap_or_default().to_owned();
    // An invalid key is answered with 400 and the reason API_KEY_INVALID.
    let invalid_key =
        error["details"].as_array().is_some_and(|details| details.iter().any(|d| d["reason"] == "API_KEY_INVALID"));
    match status {
        400 if invalid_key => AiError::Unauthorized,
        401 | 403 => AiError::Unauthorized,
        429 => AiError::RateLimited,
        500 | 502 | 503 | 504 => AiError::Overloaded,
        _ => AiError::Api { status, message },
    }
}

#[derive(Debug, Default)]
struct Stream {
    text: String,
    finish_reason: Option<String>,
    blocked: bool,
    usage: Usage,
    model: String,
}

/// Reads the server-sent events; each one is a `GenerateContentResponse`.
fn read_stream(reader: impl BufRead, on_progress: &mut impl FnMut(usize)) -> Result<Stream, AiError> {
    let mut stream = Stream::default();
    for line in reader.lines() {
        let line = line.map_err(|e| AiError::Network(e.to_string()))?;
        let Some(data) = line.strip_prefix("data:") else { continue };
        let Ok(chunk) = serde_json::from_str::<Value>(data.trim()) else { continue };
        if let Some(error) = chunk.get("error") {
            let message = error["message"].as_str().unwrap_or_default().to_owned();
            return Err(match error["status"].as_str() {
                Some("RESOURCE_EXHAUSTED") => AiError::RateLimited,
                Some("UNAVAILABLE") => AiError::Overloaded,
                _ => AiError::Api { status: 200, message },
            });
        }
        if chunk["promptFeedback"]["blockReason"].as_str().is_some_and(|r| !r.is_empty()) {
            stream.blocked = true;
        }
        if let Some(model) = chunk["modelVersion"].as_str() {
            stream.model = model.to_owned();
        }
        if let Some(candidate) = chunk["candidates"].get(0) {
            for part in candidate["content"]["parts"].as_array().into_iter().flatten() {
                // Thought summaries are not part of the answer.
                if part["thought"] == true {
                    continue;
                }
                if let Some(text) = part["text"].as_str() {
                    stream.text.push_str(text);
                    on_progress(stream.text.len());
                }
            }
            if let Some(reason) = candidate["finishReason"].as_str() {
                stream.finish_reason = Some(reason.to_owned());
            }
        }
        let usage = &chunk["usageMetadata"];
        if usage.is_object() {
            stream.usage.input_tokens = usage["promptTokenCount"].as_u64().unwrap_or(0);
            // Thinking is billed as output.
            stream.usage.output_tokens =
                usage["candidatesTokenCount"].as_u64().unwrap_or(0) + usage["thoughtsTokenCount"].as_u64().unwrap_or(0);
            stream.usage.cache_read_tokens = usage["cachedContentTokenCount"].as_u64().unwrap_or(0);
        }
    }
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;

    fn config(effort: &str) -> Config {
        Config {
            provider: Provider::Gemini,
            api_key: "k".into(),
            model: Provider::Gemini.default_model().into(),
            effort: effort.into(),
        }
    }

    #[test]
    fn body_uses_json_schema_system_instruction_and_thinking() {
        let body = request_body(&config("medium"), "sys", "user");
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "sys");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "user");
        let format = &body["generationConfig"]["responseFormat"]["text"];
        assert_eq!(format["mimeType"], "APPLICATION_JSON");
        assert!(format["schema"]["properties"]["screens"].is_object());
        assert_eq!(body["generationConfig"]["thinkingConfig"]["thinkingLevel"], "MEDIUM");
        let max = request_body(&config("max"), "sys", "user");
        assert_eq!(max["generationConfig"]["thinkingConfig"]["thinkingLevel"], "HIGH");
        let fast = request_body(&config("low"), "sys", "user");
        assert_eq!(fast["generationConfig"]["thinkingConfig"]["thinkingLevel"], "LOW");
    }

    #[test]
    fn url_names_the_model_and_asks_for_events() {
        assert_eq!(
            api_url("models/gemini-4-carbon "),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-4-carbon:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn parses_event_stream() {
        let events = "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"plan\",\"thought\":true},{\"text\":\"{\\\"summary\\\":\"}]}}],\"modelVersion\":\"gemini-3.8-flash\"}\n\n\
data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"\\\"ok\\\",\\\"screens\\\":[]}\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":5,\"thoughtsTokenCount\":2,\"cachedContentTokenCount\":8},\"modelVersion\":\"gemini-3.8-flash\"}\n\n";
        let mut progress = 0;
        let stream = read_stream(events.as_bytes(), &mut |n| progress = n).unwrap();
        assert_eq!(stream.text, r#"{"summary":"ok","screens":[]}"#);
        assert_eq!(stream.finish_reason.as_deref(), Some("STOP"));
        assert_eq!(stream.usage, Usage { input_tokens: 10, output_tokens: 7, cache_read_tokens: 8 });
        assert_eq!(stream.model, "gemini-3.8-flash");
        assert_eq!(progress, stream.text.len());
    }

    #[test]
    fn blocked_prompts_and_stream_errors_are_reported() {
        let blocked = "data: {\"promptFeedback\":{\"blockReason\":\"SAFETY\"}}\n";
        assert!(read_stream(blocked.as_bytes(), &mut |_| {}).unwrap().blocked);
        let busy = "data: {\"error\":{\"code\":503,\"message\":\"overloaded\",\"status\":\"UNAVAILABLE\"}}\n";
        assert!(matches!(read_stream(busy.as_bytes(), &mut |_| {}), Err(AiError::Overloaded)));
    }

    #[test]
    fn missing_key_fails_early() {
        let config = Config { api_key: " ".into(), ..config("medium") };
        assert!(matches!(generate(&config, "", "", |_| {}), Err(AiError::MissingKey)));
    }
}
