//! Client for services with the OpenAI Chat Completions format (OpenAI,
//! DeepSeek). Streams the response like the Claude client.

use std::io::{BufRead, BufReader};
use std::time::Duration;

use serde_json::{Value, json};

use crate::client::{AiError, Answer, Config, Usage};
use crate::provider::Provider;
use crate::spec::{Design, schema};

const MAX_TOKENS: u32 = 64000;

/// Chat Completions endpoint of a provider.
pub fn api_url(provider: Provider) -> &'static str {
    match provider {
        Provider::DeepSeek => "https://api.deepseek.com/chat/completions",
        _ => "https://api.openai.com/v1/chat/completions",
    }
}

/// Builds the request body. Kept separate for tests.
pub fn request_body(config: &Config, system: &str, user: &str) -> Value {
    let provider = config.provider;
    let mut system = system.to_owned();
    if !provider.enforces_schema() {
        // JSON mode only guarantees valid JSON, so the schema goes into the prompt.
        system.push_str(
            "\nAnswer with one JSON object that matches this JSON schema exactly (no Markdown, no other text):\n",
        );
        system.push_str(&schema().to_string());
        system.push('\n');
    }
    let mut body = json!({
        "model": config.model,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ]
    });
    if provider.enforces_schema() {
        body["response_format"] =
            json!({ "type": "json_schema", "json_schema": { "name": "design", "strict": true, "schema": schema() } });
        body["max_completion_tokens"] = json!(MAX_TOKENS);
    } else {
        body["response_format"] = json!({ "type": "json_object" });
        body["max_tokens"] = json!(MAX_TOKENS);
    }
    // DeepSeek thinks by default and maps the levels itself
    // (low → low, medium/high/xhigh → high, max → max).
    if !config.effort.is_empty() {
        body["reasoning_effort"] = json!(config.effort);
    }
    body
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
        .post(api_url(config.provider))
        .set("content-type", "application/json")
        .set("authorization", &format!("Bearer {}", config.api_key.trim()));
    let response = match request.send_json(request_body(config, system, user)) {
        Ok(response) => response,
        Err(ureq::Error::Status(status, response)) => return Err(status_error(status, response)),
        Err(err) => return Err(AiError::Network(err.to_string())),
    };
    let stream = read_stream(BufReader::new(response.into_reader()), &mut on_progress)?;
    if stream.refused {
        return Err(AiError::Refused);
    }
    match stream.finish_reason.as_deref() {
        Some("content_filter") => return Err(AiError::Refused),
        Some("length") => return Err(AiError::Truncated),
        _ => {}
    }
    let design = parse_design(&stream.text)?;
    let model = if stream.model.is_empty() { config.model.clone() } else { stream.model };
    Ok(Answer { design, usage: stream.usage, model })
}

/// Parses the answer; tolerates a Markdown code fence around the JSON.
fn parse_design(text: &str) -> Result<Design, AiError> {
    let mut text = text.trim();
    if let Some(rest) = text.strip_prefix("```") {
        text = rest.trim_start_matches("json").trim_start();
        text = text.strip_suffix("```").unwrap_or(text).trim();
    }
    if text.is_empty() {
        return Err(AiError::InvalidAnswer("empty answer".to_owned()));
    }
    serde_json::from_str::<Design>(text).map_err(|e| AiError::InvalidAnswer(e.to_string()))
}

fn status_error(status: u16, response: ureq::Response) -> AiError {
    let error = response.into_json::<Value>().ok().map(|v| v["error"].clone()).unwrap_or_default();
    let message = error["message"].as_str().unwrap_or_default().to_owned();
    match status {
        401 | 403 => AiError::Unauthorized,
        // An exhausted quota also answers 429; its message says what to do.
        429 if error["code"] == "insufficient_quota" => AiError::Api { status, message },
        429 => AiError::RateLimited,
        500 | 502 | 503 | 529 => AiError::Overloaded,
        _ => AiError::Api { status, message },
    }
}

#[derive(Debug, Default)]
struct Stream {
    text: String,
    finish_reason: Option<String>,
    refused: bool,
    usage: Usage,
    model: String,
}

/// Reads the server-sent events of a streamed chat completion.
fn read_stream(reader: impl BufRead, on_progress: &mut impl FnMut(usize)) -> Result<Stream, AiError> {
    let mut stream = Stream::default();
    for line in reader.lines() {
        let line = line.map_err(|e| AiError::Network(e.to_string()))?;
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else { continue };
        if let Some(error) = chunk.get("error") {
            let message = error["message"].as_str().unwrap_or_default().to_owned();
            return Err(AiError::Api { status: 200, message });
        }
        if let Some(model) = chunk["model"].as_str() {
            stream.model = model.to_owned();
        }
        if let Some(choice) = chunk["choices"].get(0) {
            let delta = &choice["delta"];
            if let Some(text) = delta["content"].as_str() {
                stream.text.push_str(text);
                on_progress(stream.text.len());
            }
            if delta["refusal"].as_str().is_some_and(|r| !r.is_empty()) {
                stream.refused = true;
            }
            if let Some(reason) = choice["finish_reason"].as_str() {
                stream.finish_reason = Some(reason.to_owned());
            }
        }
        let usage = &chunk["usage"];
        if usage.is_object() {
            stream.usage.input_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0);
            stream.usage.output_tokens = usage["completion_tokens"].as_u64().unwrap_or(0);
            // OpenAI reports cached tokens in the details, DeepSeek as cache hits.
            stream.usage.cache_read_tokens = usage["prompt_tokens_details"]["cached_tokens"]
                .as_u64()
                .or_else(|| usage["prompt_cache_hit_tokens"].as_u64())
                .unwrap_or(0);
        }
    }
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(provider: Provider) -> Config {
        Config { provider, api_key: "k".into(), model: provider.default_model().into(), effort: "high".into() }
    }

    #[test]
    fn openai_body_uses_strict_schema_and_effort() {
        let body = request_body(&config(Provider::OpenAi), "sys", "user");
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body["messages"][0]["content"], "sys");
        assert_eq!(body["messages"][1]["role"], "user");
        assert!(body.get("max_completion_tokens").is_some());
        assert_eq!(api_url(Provider::OpenAi), "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn deepseek_body_uses_json_mode_with_schema_in_prompt() {
        let body = request_body(&config(Provider::DeepSeek), "sys", "user");
        assert_eq!(body["response_format"]["type"], "json_object");
        assert_eq!(body["reasoning_effort"], "high");
        assert!(body.get("max_tokens").is_some());
        let system = body["messages"][0]["content"].as_str().unwrap();
        // JSON mode needs the word "json" in the prompt.
        assert!(system.to_lowercase().contains("json"));
        assert!(system.contains("\"screens\""));
        assert_eq!(api_url(Provider::DeepSeek), "https://api.deepseek.com/chat/completions");
    }

    #[test]
    fn parses_event_stream() {
        let events = "data: {\"model\":\"gpt-6.2-sol\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n\
data: {\"model\":\"gpt-6.2-sol\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"{\\\"summary\\\":\"},\"finish_reason\":null}]}\n\n\
data: {\"model\":\"gpt-6.2-sol\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"\\\"ok\\\",\\\"screens\\\":[]}\"},\"finish_reason\":null}]}\n\n\
data: {\"model\":\"gpt-6.2-sol\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
data: {\"model\":\"gpt-6.2-sol\",\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"prompt_tokens_details\":{\"cached_tokens\":8}}}\n\n\
data: [DONE]\n\n";
        let mut progress = 0;
        let stream = read_stream(events.as_bytes(), &mut |n| progress = n).unwrap();
        assert_eq!(stream.text, r#"{"summary":"ok","screens":[]}"#);
        assert_eq!(stream.finish_reason.as_deref(), Some("stop"));
        assert_eq!(stream.usage, Usage { input_tokens: 10, output_tokens: 5, cache_read_tokens: 8 });
        assert_eq!(stream.model, "gpt-6.2-sol");
        assert_eq!(progress, stream.text.len());
        assert_eq!(parse_design(&stream.text).unwrap().summary, "ok");
    }

    #[test]
    fn deepseek_usage_and_reasoning_are_read() {
        let events = "data: {\"model\":\"deepseek-flash\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"hmm\"},\"finish_reason\":null}]}\n\n\
data: {\"model\":\"deepseek-flash\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"{}\"},\"finish_reason\":\"length\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3,\"prompt_cache_hit_tokens\":4}}\n\n";
        let stream = read_stream(events.as_bytes(), &mut |_| {}).unwrap();
        assert_eq!(stream.text, "{}", "reasoning is not part of the answer");
        assert_eq!(stream.finish_reason.as_deref(), Some("length"));
        assert_eq!(stream.usage.cache_read_tokens, 4);
    }

    #[test]
    fn refusals_and_stream_errors_are_reported() {
        let refusal =
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"refusal\":\"I can't\"},\"finish_reason\":null}]}\n";
        assert!(read_stream(refusal.as_bytes(), &mut |_| {}).unwrap().refused);
        let error = "data: {\"error\":{\"message\":\"boom\"}}\n";
        assert!(matches!(read_stream(error.as_bytes(), &mut |_| {}), Err(AiError::Api { .. })));
    }

    #[test]
    fn code_fences_are_tolerated() {
        let design = parse_design("```json\n{\"summary\":\"s\",\"screens\":[]}\n```").unwrap();
        assert_eq!(design.summary, "s");
        assert!(matches!(parse_design("  "), Err(AiError::InvalidAnswer(_))));
    }

    #[test]
    fn missing_key_fails_early() {
        let config = Config { api_key: " ".into(), ..config(Provider::DeepSeek) };
        assert!(matches!(generate(&config, "", "", |_| {}), Err(AiError::MissingKey)));
    }
}
