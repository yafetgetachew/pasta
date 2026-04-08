#[cfg(target_os = "macos")]
use crate::*;

#[cfg(target_os = "macos")]
fn ui_style_state_path() -> Option<PathBuf> {
    let base = dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .or_else(dirs::home_dir)?;
    let directory = base.join("PastaClipboard");
    if let Err(err) = fs::create_dir_all(&directory) {
        eprintln!("warning: unable to create config directory '{directory:?}': {err}");
        return None;
    }
    Some(directory.join("ui-style.json"))
}

#[cfg(target_os = "macos")]
fn default_ui_style_state(default_family: SharedString) -> UiStyleState {
    UiStyleState {
        family: default_family,
        surface_alpha: 0.90,
        syntax_highlighting: true,
        secret_auto_clear: true,
        pasta_brain_enabled: true,
    }
}

#[cfg(target_os = "macos")]
fn load_ui_style_state(default_family: SharedString) -> UiStyleState {
    let mut style = default_ui_style_state(default_family);
    let Some(path) = ui_style_state_path() else {
        return style;
    };

    let data = match fs::read_to_string(&path) {
        Ok(data) => data,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return style,
        Err(err) => {
            eprintln!("warning: unable to read style settings from '{path:?}': {err}");
            return style;
        }
    };

    let persisted: PersistedUiStyleState = match serde_json::from_str(&data) {
        Ok(persisted) => persisted,
        Err(err) => {
            eprintln!("warning: unable to parse style settings from '{path:?}': {err}");
            return style;
        }
    };

    let family = persisted.family.trim();
    if !family.is_empty() {
        style.family = family.to_owned().into();
    }
    style.surface_alpha = persisted.surface_alpha.clamp(0.45, 1.0);
    style.syntax_highlighting = persisted.syntax_highlighting;
    style.secret_auto_clear = persisted.secret_auto_clear;
    style.pasta_brain_enabled = persisted.pasta_brain_enabled;
    style
}

#[cfg(target_os = "macos")]
fn save_ui_style_state(style: &UiStyleState) {
    let Some(path) = ui_style_state_path() else {
        return;
    };

    let serialized = match serde_json::to_string_pretty(&PersistedUiStyleState {
        family: style.family.to_string(),
        surface_alpha: style.surface_alpha.clamp(0.45, 1.0),
        syntax_highlighting: style.syntax_highlighting,
        secret_auto_clear: style.secret_auto_clear,
        pasta_brain_enabled: style.pasta_brain_enabled,
    }) {
        Ok(serialized) => serialized,
        Err(err) => {
            eprintln!("warning: unable to serialize style settings: {err}");
            return;
        }
    };

    if let Err(err) = fs::write(&path, serialized) {
        eprintln!("warning: unable to write style settings to '{path:?}': {err}");
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn persist_ui_style_state(cx: &App) {
    save_ui_style_state(cx.global::<UiStyleState>());
}

#[cfg(target_os = "macos")]
pub(crate) fn load_embedded_ui_font(cx: &mut App) {
    let font_blobs: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../../../assets/fonts/MesloLGSNF-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/MesloLGSNF-Bold.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/MesloLGSNF-Italic.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/MesloLGSNF-BoldItalic.ttf").as_slice()),
        Cow::Borrowed(
            include_bytes!("../../../assets/fonts/IosevkaTermNerdFont-Light.ttf").as_slice(),
        ),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/IBMPlexMono-Light.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/JetBrainsMono-Light.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/SourceCodePro-Var.ttf").as_slice()),
        Cow::Borrowed(
            include_bytes!("../../../assets/fonts/MonaspaceNeonFrozen-Light.ttf").as_slice(),
        ),
    ];

    if let Err(err) = cx.text_system().add_fonts(font_blobs) {
        eprintln!("warning: unable to load embedded Meslo font: {err}");
    }

    let default_family =
        resolve_font_family(cx, FontChoice::MesloLg).unwrap_or_else(|| "Menlo".into());
    cx.set_global(load_ui_style_state(default_family));
}

#[cfg(target_os = "macos")]
pub(crate) fn resolve_font_family(cx: &App, choice: FontChoice) -> Option<SharedString> {
    let all_names = cx.text_system().all_font_names();
    let all_normalized: Vec<String> = all_names
        .iter()
        .map(|name| normalize_font_name(name))
        .collect();

    for candidate in choice.candidates() {
        let candidate_normalized = normalize_font_name(candidate);
        if candidate_normalized.is_empty() {
            continue;
        }

        if let Some((ix, _)) = all_normalized
            .iter()
            .enumerate()
            .find(|(_, name)| *name == &candidate_normalized)
        {
            return Some(all_names[ix].clone().into());
        }
    }

    for candidate in choice.candidates() {
        let candidate_normalized = normalize_font_name(candidate);
        if candidate_normalized.is_empty() {
            continue;
        }

        if let Some((ix, _)) = all_normalized.iter().enumerate().find(|(_, name)| {
            name.contains(&candidate_normalized) || candidate_normalized.contains(*name)
        }) {
            return Some(all_names[ix].clone().into());
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn normalize_font_name(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

#[cfg(target_os = "macos")]
pub(crate) fn apply_style_to_open_window(cx: &mut App) {
    let style = cx.global::<UiStyleState>().clone();
    if let Some(window) = cx
        .try_global::<LauncherState>()
        .and_then(|state| state.window)
    {
        let _ = window.update(cx, |view, _window, cx| {
            view.font_family = style.family.clone();
            view.surface_alpha = style.surface_alpha;
            view.syntax_highlighting = style.syntax_highlighting;
            cx.notify();
        });
    }
}
