use nsh_core::{platform::Appearance, prelude::*, storage::Storage};
use serde::{Deserialize, Serialize};

const KEY: &str = "bas.preferences";
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Preferences {
    pub language: String,
    pub appearance: String,
    pub animations: bool,
    pub ui_size: f32,
    pub content_size: f32,
    pub code_size: f32,
    pub ui_font: String,
    pub code_font: String,
    pub line_height: f32,
    pub background_opacity: f32,
    pub sidebar_opacity: f32,
    pub follow_messages: bool,
    pub show_thoughts: bool,
    pub expand_tools: bool,
    pub wrap_output: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: "system".into(),
            appearance: "system".into(),
            animations: true,
            ui_size: 16.0,
            content_size: 14.0,
            code_size: 13.0,
            ui_font: String::new(),
            code_font: String::new(),
            line_height: 1.6,
            background_opacity: 100.0,
            sidebar_opacity: 100.0,
            follow_messages: true,
            show_thoughts: true,
            expand_tools: false,
            wrap_output: true,
        }
    }
}
pub(crate) fn state() -> Signal<Preferences> {
    global::<Preferences>()
}
pub(crate) fn error() -> Signal<String> {
    global::<String>()
}
pub(crate) fn initialize() {
    if let Some(value) = Storage::get_item(KEY).and_then(|s| serde_json::from_str(&s).ok()) {
        state().set(value);
    }
    apply();
}
fn apply() {
    let p = state().get_untracked();
    set_color_scheme(match p.appearance.as_str() {
        "light" => Appearance::Light,
        "dark" => Appearance::Dark,
        _ => Appearance::System,
    });
    let mut json: serde_json::Value =
        serde_json::from_str(include_str!("preferences/assets/theme.json")).expect("bundled theme");
    if !p.ui_font.is_empty() {
        json["cssVars"]["theme"]["font-sans"] = p.ui_font.into();
    }
    if !p.code_font.is_empty() {
        json["cssVars"]["theme"]["font-mono"] = p.code_font.into();
    }
    set_tokens(nsh_base::tokens_of(Some(&json.to_string())).expect("bundled theme tokens"));
}
pub(crate) fn change(edit: impl FnOnce(&mut Preferences)) {
    state().update(edit);
    apply();
    let json = serde_json::to_string(&state().get_untracked()).expect("preferences serialize");
    error().set(
        Storage::set_item(KEY, json)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default(),
    );
}
pub(crate) fn reset_appearance() {
    change(|p| {
        let d = Preferences::default();
        p.appearance = d.appearance;
        p.ui_size = d.ui_size;
        p.content_size = d.content_size;
        p.code_size = d.code_size;
        p.ui_font = d.ui_font;
        p.code_font = d.code_font;
        p.line_height = d.line_height;
        p.background_opacity = d.background_opacity;
        p.sidebar_opacity = d.sidebar_opacity;
        p.animations = d.animations;
    });
}
