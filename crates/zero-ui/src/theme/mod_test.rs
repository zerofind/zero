use gpui_component::theme::{ThemeMode, ThemeSet};

use super::ZERO_THEME;

#[test]
fn zero_theme_parses_with_both_modes() {
    let set: ThemeSet = serde_json::from_str(ZERO_THEME).expect("zero.json should parse");

    assert!(
        set.themes.iter().any(|t| t.mode == ThemeMode::Dark),
        "must contain a Dark variant"
    );
    assert!(
        set.themes.iter().any(|t| t.mode == ThemeMode::Light),
        "must contain a Light variant"
    );
}

#[test]
fn zero_theme_names_are_prefixed() {
    let set: ThemeSet = serde_json::from_str(ZERO_THEME).unwrap();
    for config in &set.themes {
        assert!(
            config.name.starts_with("Zero"),
            "theme name '{}' should start with 'Zero'",
            config.name
        );
    }
}
