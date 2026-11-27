//! Per-user application preferences, persisted by eframe.

use std::path::PathBuf;

use lvglab_model::project::{DisplaySettings, LvglVersion, ThemeSettings};
use serde::{Deserialize, Serialize};

use crate::i18n::Language;

pub const STORAGE_KEY: &str = "lvglab.settings";
const MAX_RECENT: usize = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    #[default]
    System,
    Dark,
    Light,
}

impl Appearance {
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }
}

/// What is drawn around the display on the canvas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CanvasBackground {
    /// Follows the editor appearance.
    #[default]
    Theme,
    Dark,
    Light,
    Checkerboard,
}

impl CanvasBackground {
    pub const ALL: [Self; 4] = [Self::Theme, Self::Dark, Self::Light, Self::Checkerboard];

    pub fn label(self) -> &'static str {
        match self {
            Self::Theme => "Like the editor",
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Checkerboard => "Checkerboard",
        }
    }
}

/// What draws the preview on the canvas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewRenderer {
    /// The editor's own drawing: instant, with every editing aid.
    #[default]
    Editor,
    /// LVGL itself renders the export of the screen (LVGL 8 projects).
    Lvgl,
}

impl PreviewRenderer {
    pub const ALL: [Self; 2] = [Self::Editor, Self::Lvgl];

    pub fn label(self) -> &'static str {
        match self {
            Self::Editor => "Editor (fast)",
            Self::Lvgl => "LVGL (exact)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    // General
    pub language: Language,
    pub appearance: Appearance,
    /// Scale of the whole editor UI (1.0 = 100 %).
    pub ui_scale: f32,
    /// Opens the most recent project when started without a file.
    pub reopen_last: bool,
    // Canvas
    pub canvas_background: CanvasBackground,
    pub preview_renderer: PreviewRenderer,
    pub grid_size: u32,
    pub show_grid: bool,
    pub snap_to_grid: bool,
    /// Snap lines to the edges and centers of neighbors while moving.
    pub smart_guides: bool,
    /// Thin outlines around every widget.
    pub show_bounds: bool,
    /// Dashed outlines for hidden widgets.
    pub show_hidden: bool,
    /// Name and size next to the selection.
    pub show_size_label: bool,
    /// Pixels moved by Shift + arrow key.
    pub nudge_large: u32,
    // Editing
    /// Maximum number of steps in the timeline.
    pub history_limit: usize,
    /// Minutes between automatic saves (0 = off).
    pub autosave_minutes: u32,
    /// Autosave writes the project file itself; otherwise a backup copy.
    pub autosave_project: bool,
    /// Keep the timeline in `<name>.timeline` next to the project.
    pub save_timeline: bool,
    // Export
    pub open_export_folder: bool,
    /// Looks for a newer release on start.
    pub check_updates: bool,
    // AI assistant
    /// The service the assistant uses.
    pub ai_provider: lvglab_ai::Provider,
    /// Anthropic API key; empty uses the `ANTHROPIC_API_KEY` environment variable.
    pub ai_api_key: String,
    /// Anthropic model.
    pub ai_model: String,
    /// OpenAI API key; empty uses the `OPENAI_API_KEY` environment variable.
    pub ai_openai_key: String,
    pub ai_openai_model: String,
    /// DeepSeek API key; empty uses the `DEEPSEEK_API_KEY` environment variable.
    pub ai_deepseek_key: String,
    pub ai_deepseek_model: String,
    pub ai_effort: String,
    pub recent: Vec<PathBuf>,
    /// Choices of the last new project, offered again.
    pub new_project: NewProjectDefaults,
    /// The welcome tour was finished or skipped.
    pub tour_done: bool,
}

/// Display, LVGL release and theme of the last project created with File → New.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewProjectDefaults {
    pub display: DisplaySettings,
    pub lvgl_version: LvglVersion,
    pub theme: ThemeSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: Language::from_system(),
            appearance: Appearance::System,
            ui_scale: 1.0,
            reopen_last: false,
            canvas_background: CanvasBackground::Theme,
            preview_renderer: PreviewRenderer::Editor,
            grid_size: 10,
            show_grid: false,
            snap_to_grid: false,
            smart_guides: true,
            show_bounds: false,
            show_hidden: true,
            show_size_label: true,
            nudge_large: 10,
            history_limit: 300,
            autosave_minutes: 2,
            autosave_project: true,
            save_timeline: true,
            open_export_folder: false,
            check_updates: true,
            ai_provider: lvglab_ai::Provider::Anthropic,
            ai_api_key: String::new(),
            ai_model: lvglab_ai::MODELS[0].0.to_owned(),
            ai_openai_key: String::new(),
            ai_openai_model: lvglab_ai::Provider::OpenAi.default_model().to_owned(),
            ai_deepseek_key: String::new(),
            ai_deepseek_model: lvglab_ai::Provider::DeepSeek.default_model().to_owned(),
            ai_effort: "medium".to_owned(),
            recent: Vec::new(),
            new_project: NewProjectDefaults::default(),
            tour_done: false,
        }
    }
}

impl AppSettings {
    pub fn add_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT);
    }

    /// Restores the defaults but keeps the recent files and the API keys.
    pub fn reset(&mut self) {
        let recent = std::mem::take(&mut self.recent);
        let ai_api_key = std::mem::take(&mut self.ai_api_key);
        let ai_openai_key = std::mem::take(&mut self.ai_openai_key);
        let ai_deepseek_key = std::mem::take(&mut self.ai_deepseek_key);
        *self = Self { recent, ai_api_key, ai_openai_key, ai_deepseek_key, ..Self::default() };
    }

    /// The configured API key of a provider (may be empty).
    pub fn key_mut(&mut self, provider: lvglab_ai::Provider) -> &mut String {
        match provider {
            lvglab_ai::Provider::Anthropic => &mut self.ai_api_key,
            lvglab_ai::Provider::OpenAi => &mut self.ai_openai_key,
            lvglab_ai::Provider::DeepSeek => &mut self.ai_deepseek_key,
        }
    }

    /// The model of a provider.
    pub fn model_mut(&mut self, provider: lvglab_ai::Provider) -> &mut String {
        match provider {
            lvglab_ai::Provider::Anthropic => &mut self.ai_model,
            lvglab_ai::Provider::OpenAi => &mut self.ai_openai_model,
            lvglab_ai::Provider::DeepSeek => &mut self.ai_deepseek_model,
        }
    }

    /// The model of the selected provider.
    pub fn model(&self) -> &str {
        match self.ai_provider {
            lvglab_ai::Provider::Anthropic => &self.ai_model,
            lvglab_ai::Provider::OpenAi => &self.ai_openai_model,
            lvglab_ai::Provider::DeepSeek => &self.ai_deepseek_model,
        }
    }

    fn configured_key(&self, provider: lvglab_ai::Provider) -> &str {
        match provider {
            lvglab_ai::Provider::Anthropic => &self.ai_api_key,
            lvglab_ai::Provider::OpenAi => &self.ai_openai_key,
            lvglab_ai::Provider::DeepSeek => &self.ai_deepseek_key,
        }
    }

    /// Whether the key of `provider` comes from its environment variable.
    pub fn key_from_env(&self, provider: lvglab_ai::Provider) -> bool {
        self.configured_key(provider).trim().is_empty()
            && std::env::var(provider.env_var()).is_ok_and(|k| !k.trim().is_empty())
    }

    /// The API key to use for the selected provider: the configured one or
    /// the provider's environment variable.
    pub fn api_key(&self) -> String {
        let provider = self.ai_provider;
        let key = self.configured_key(provider).trim();
        if key.is_empty() {
            std::env::var(provider.env_var()).unwrap_or_default().trim().to_owned()
        } else {
            key.to_owned()
        }
    }

    /// Request settings for the assistant.
    pub fn ai_config(&self) -> lvglab_ai::Config {
        lvglab_ai::Config {
            provider: self.ai_provider,
            api_key: self.api_key(),
            model: self.model().trim().to_owned(),
            effort: self.ai_effort.clone(),
        }
    }

    /// Clamps values that may come from an older or edited settings file.
    pub fn sanitize(&mut self) {
        self.ui_scale = self.ui_scale.clamp(0.6, 2.0);
        self.grid_size = self.grid_size.clamp(2, 200);
        self.nudge_large = self.nudge_large.clamp(2, 200);
        self.history_limit = self.history_limit.clamp(20, 5000);
        self.autosave_minutes = self.autosave_minutes.min(120);
        for provider in lvglab_ai::Provider::ALL {
            let model = self.model_mut(provider);
            let known = provider.models().iter().any(|(id, _)| *id == model.as_str());
            if model.trim().is_empty() || (!provider.custom_models() && !known) {
                *model = provider.default_model().to_owned();
            }
        }
        if !lvglab_ai::EFFORTS.contains(&self.ai_effort.as_str()) {
            self.ai_effort = "medium".to_owned();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_keeps_recent_files() {
        let mut settings = AppSettings { show_bounds: true, ..AppSettings::default() };
        settings.add_recent(PathBuf::from("a.lvglab"));
        settings.reset();
        assert!(!settings.show_bounds);
        assert_eq!(settings.recent.len(), 1);
    }

    #[test]
    fn sanitize_clamps_values() {
        let mut settings = AppSettings { ui_scale: 9.0, history_limit: 1, ..AppSettings::default() };
        settings.sanitize();
        assert_eq!(settings.ui_scale, 2.0);
        assert_eq!(settings.history_limit, 20);
    }

    #[test]
    fn old_settings_files_still_load() {
        let settings: AppSettings =
            serde_json::from_str(r#"{"grid_size": 8, "ai_api_key": "sk-ant-x", "ai_model": "claude-haiku-5-5"}"#)
                .unwrap();
        assert_eq!(settings.grid_size, 8);
        assert_eq!(settings.history_limit, 300);
        assert_eq!(settings.ai_provider, lvglab_ai::Provider::Anthropic);
        assert_eq!(settings.ai_config().api_key, "sk-ant-x");
        assert_eq!(settings.ai_config().model, "claude-haiku-5-5");
    }

    #[test]
    fn each_provider_keeps_its_own_key_and_model() {
        let mut settings = AppSettings { ai_api_key: "anthropic".into(), ..AppSettings::default() };
        *settings.key_mut(lvglab_ai::Provider::OpenAi) = "openai".into();
        *settings.model_mut(lvglab_ai::Provider::OpenAi) = "my-custom-gpt".into();
        settings.ai_provider = lvglab_ai::Provider::OpenAi;
        settings.sanitize();
        let config = settings.ai_config();
        assert_eq!((config.api_key.as_str(), config.model.as_str()), ("openai", "my-custom-gpt"));
        settings.ai_provider = lvglab_ai::Provider::DeepSeek;
        assert_eq!(settings.ai_config().model, lvglab_ai::Provider::DeepSeek.default_model());
        settings.reset();
        assert_eq!(settings.ai_openai_key, "openai", "reset keeps the keys");
        assert_eq!(settings.ai_api_key, "anthropic");
    }

    #[test]
    fn sanitize_restores_unknown_anthropic_models() {
        let mut settings =
            AppSettings { ai_model: "gone".into(), ai_deepseek_model: " ".into(), ..AppSettings::default() };
        settings.sanitize();
        assert_eq!(settings.ai_model, lvglab_ai::MODELS[0].0);
        assert_eq!(settings.ai_deepseek_model, lvglab_ai::Provider::DeepSeek.default_model());
    }
}
