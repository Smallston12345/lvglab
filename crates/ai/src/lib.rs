//! AI assistant of LVGLab: describes the project to a model (Claude, GPT or
//! DeepSeek), asks for a UI design in a fixed JSON schema and applies the
//! answer to the project.
//!
//! * [`prompt`] builds the cached system prompt (widget catalog reference)
//!   and the per-request context.
//! * [`client`] talks to the Claude Messages API, [`openai`] to services with
//!   the OpenAI Chat Completions format; [`provider`] lists the services.
//! * [`spec`] / [`convert`] define the exchanged format and turn it into
//!   validated widgets.

pub mod client;
pub mod convert;
pub mod openai;
pub mod prompt;
pub mod provider;
pub mod spec;

pub use client::{AiError, Answer, Config, EFFORTS, MODELS, Usage};
pub use convert::{Applied, apply};
pub use prompt::Scope;
pub use provider::Provider;
pub use spec::Design;

/// Runs one request: builds the prompt for `scope`, calls the model and
/// returns its design (not yet applied).
pub fn request(
    config: &Config,
    project: &lvglab_model::Project,
    scope: Scope,
    text: &str,
    on_progress: impl FnMut(usize),
) -> Result<Answer, AiError> {
    let system = prompt::system_prompt();
    let user = prompt::user_message(project, scope, text);
    match config.provider {
        Provider::Anthropic => client::generate(config, &system, &user, on_progress),
        Provider::OpenAi | Provider::DeepSeek => openai::generate(config, &system, &user, on_progress),
    }
}
