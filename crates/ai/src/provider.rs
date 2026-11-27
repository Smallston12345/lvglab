//! The AI services the assistant can talk to.

use serde::{Deserialize, Serialize};

/// An AI service. Anthropic uses its own Messages API; the others use the
/// OpenAI Chat Completions format (see [`crate::openai`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "deepseek")]
    DeepSeek,
}

impl Provider {
    pub const ALL: [Self; 3] = [Self::Anthropic, Self::OpenAi, Self::DeepSeek];

    /// Name of the company or service, shown in the editor.
    pub fn name(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic (Claude)",
            Self::OpenAi => "OpenAI (GPT)",
            Self::DeepSeek => "DeepSeek",
        }
    }

    /// Short name for texts such as "Project data is sent to …".
    pub fn company(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic",
            Self::OpenAi => "OpenAI",
            Self::DeepSeek => "DeepSeek",
        }
    }

    /// Models offered in the editor, most capable first. Other model IDs
    /// can be typed in for every provider except Anthropic.
    pub fn models(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Anthropic => crate::client::MODELS,
            Self::OpenAi => &[("gpt-6.2-astra", "GPT-6.2 Astra"), ("gpt-6.2-sol", "GPT-6.2 Sol")],
            Self::DeepSeek => &[("deepseek-v4-pro", "DeepSeek V4 Pro"), ("deepseek-flash", "DeepSeek Flash")],
        }
    }

    pub fn default_model(self) -> &'static str {
        self.models()[0].0
    }

    /// Display name of a model ID; unknown (typed in) IDs are shown as they are.
    pub fn model_name(self, model: &str) -> String {
        self.models()
            .iter()
            .find(|(id, _)| *id == model)
            .map_or_else(|| model.to_owned(), |(_, name)| (*name).to_owned())
    }

    /// Whether model IDs outside [`Self::models`] are accepted.
    pub fn custom_models(self) -> bool {
        self != Self::Anthropic
    }

    /// Whether the effort setting is sent with the request.
    pub fn uses_effort(self) -> bool {
        self != Self::DeepSeek
    }

    /// Whether the service enforces the answer's JSON schema. Without it the
    /// schema is described in the prompt and answers fail more often.
    pub fn enforces_schema(self) -> bool {
        self != Self::DeepSeek
    }

    /// Environment variable read when no key is configured.
    pub fn env_var(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenAi => "OPENAI_API_KEY",
            Self::DeepSeek => "DEEPSEEK_API_KEY",
        }
    }

    /// Page where an API key is created.
    pub fn key_url(self) -> &'static str {
        match self {
            Self::Anthropic => "https://console.anthropic.com/settings/keys",
            Self::OpenAi => "https://platform.openai.com/api-keys",
            Self::DeepSeek => "https://platform.deepseek.com/api_keys",
        }
    }

    /// Placeholder for the key field.
    pub fn key_hint(self) -> &'static str {
        match self {
            Self::Anthropic => "sk-ant-…",
            Self::OpenAi | Self::DeepSeek => "sk-…",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_has_models_and_a_distinct_key_variable() {
        for provider in Provider::ALL {
            assert!(!provider.models().is_empty(), "{provider:?}");
            assert_eq!(provider.model_name(provider.default_model()), provider.models()[0].1);
        }
        let vars: std::collections::HashSet<_> = Provider::ALL.iter().map(|p| p.env_var()).collect();
        assert_eq!(vars.len(), Provider::ALL.len());
    }

    #[test]
    fn serializes_with_stable_names() {
        let names: Vec<String> = Provider::ALL.iter().map(|p| serde_json::to_string(p).unwrap()).collect();
        assert_eq!(names, ["\"anthropic\"", "\"openai\"", "\"deepseek\""]);
        assert_eq!(Provider::OpenAi.model_name("my-model"), "my-model");
    }
}
