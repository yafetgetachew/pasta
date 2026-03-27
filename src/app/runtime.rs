#[cfg(target_os = "macos")]
use crate::*;
#[cfg(target_os = "macos")]
use futures::StreamExt;

pub(crate) struct HotkeyRegistration {
    pub(crate) _manager: GlobalHotKeyManager,
    pub(crate) hotkey_id: u32,
}

#[cfg(target_os = "macos")]
impl Global for HotkeyRegistration {}

#[cfg(target_os = "macos")]
#[derive(Default)]
pub(crate) struct LauncherState {
    pub(crate) window: Option<WindowHandle<LauncherView>>,
}

#[cfg(target_os = "macos")]
impl Global for LauncherState {}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct PendingAutoClear {
    pub(crate) due_at: Instant,
    pub(crate) expected_hash: String,
}

#[cfg(target_os = "macos")]
#[derive(Default)]
pub(crate) struct AutoClearState {
    pub(crate) pending: Option<PendingAutoClear>,
}

#[cfg(target_os = "macos")]
impl Global for AutoClearState {}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(crate) struct PendingSelfClipboardWrite {
    pub(crate) due_at: Instant,
    pub(crate) expected_hash: String,
}

#[cfg(target_os = "macos")]
#[derive(Default)]
pub(crate) struct SelfClipboardWriteState {
    pub(crate) pending: Option<PendingSelfClipboardWrite>,
}

#[cfg(target_os = "macos")]
impl Global for SelfClipboardWriteState {}

#[cfg(target_os = "macos")]
pub(crate) struct StatusItemRegistration {
    pub(crate) _status_item: StrongPtr,
    pub(crate) _menu: StrongPtr,
    pub(crate) _handler: StrongPtr,
}

#[cfg(target_os = "macos")]
impl Global for StatusItemRegistration {}

#[cfg(target_os = "macos")]
fn handle_menu_command(command: MenuCommand, cx: &mut App) {
    match command {
        MenuCommand::ShowLauncher => show_launcher(cx),
        MenuCommand::QuitApp => {
            let mut should_terminate_now = true;
            if let Some(window) = cx
                .try_global::<LauncherState>()
                .and_then(|state| state.window)
                && window.is_active(cx).unwrap_or(false)
            {
                should_terminate_now = false;
                let _ = window.update(cx, |view, _window, cx| {
                    view.begin_close_transition(LauncherExitIntent::Quit);
                    cx.notify();
                });
            }

            if should_terminate_now {
                cx.quit();
            }
        }
        MenuCommand::SetFont(choice) => {
            if let Some(family) = resolve_font_family(cx, choice) {
                cx.global_mut::<UiStyleState>().family = family;
                apply_style_to_open_window(cx);
                persist_ui_style_state(cx);
            } else {
                let fallback = choice
                    .candidates()
                    .first()
                    .copied()
                    .unwrap_or_else(|| choice.label());
                cx.global_mut::<UiStyleState>().family = fallback.into();
                apply_style_to_open_window(cx);
                persist_ui_style_state(cx);
                eprintln!(
                    "warning: requested font '{}' not resolved via text system; using fallback '{}'",
                    choice.label(),
                    fallback
                );
            }
        }
        MenuCommand::SetTransparency(alpha) => {
            cx.global_mut::<UiStyleState>().surface_alpha = alpha.clamp(0.45, 1.0);
            apply_style_to_open_window(cx);
            persist_ui_style_state(cx);
        }
        MenuCommand::SetSyntaxHighlighting(enabled) => {
            cx.global_mut::<UiStyleState>().syntax_highlighting = enabled;
            apply_style_to_open_window(cx);
            persist_ui_style_state(cx);
        }
        MenuCommand::SetSecretAutoClear(enabled) => {
            cx.global_mut::<UiStyleState>().secret_auto_clear = enabled;
            persist_ui_style_state(cx);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn spawn_menu_command_listener(cx: &mut App, receiver: mpsc::Receiver<MenuCommand>) {
    cx.spawn(async move |cx| {
        loop {
            while let Ok(command) = receiver.try_recv() {
                let _ = cx.update(|cx| {
                    handle_menu_command(command, cx);
                });
            }

            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
        }
    })
    .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn spawn_search_result_listener(
    cx: &mut App,
    window: WindowHandle<LauncherView>,
    receiver: futures::channel::mpsc::UnboundedReceiver<SearchResponse>,
) {
    cx.spawn(async move |cx| {
        let mut receiver = receiver;
        while let Some(response) = receiver.next().await {
            let update_result = cx.update(|cx| {
                window
                    .update(cx, |view, _window, cx| {
                        if view.apply_search_response(response) {
                            cx.notify();
                        }
                    })
                    .is_ok()
            });
            if !matches!(update_result, Ok(true)) {
                break;
            }
        }
    })
    .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn spawn_launcher_transition_loop(cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            let _ = cx.update(|cx| {
                if let Some(window) = cx
                    .try_global::<LauncherState>()
                    .and_then(|state| state.window)
                {
                    let _ = window.update(cx, |view, window, cx| {
                        let appearance_changed = view.sync_window_appearance(window);
                        let reveal_changed = view.clear_expired_secret_reveal();
                        let reveal_tick_changed = view.secret_countdown_tick_changed();
                        let transition_active = view.transition_running();

                        if !transition_active {
                            if appearance_changed || reveal_changed || reveal_tick_changed {
                                cx.notify();
                            }
                            return;
                        }

                        let maybe_exit = view.tick_transition();
                        cx.notify();

                        match maybe_exit {
                            Some(LauncherExitIntent::Hide) => cx.hide(),
                            Some(LauncherExitIntent::Quit) => cx.quit(),
                            None => {}
                        }
                    });
                }
            });

            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
        }
    })
    .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn show_launcher(cx: &mut App) {
    cx.activate(true);
    let style = cx.global::<UiStyleState>().clone();

    let mut window = cx
        .try_global::<LauncherState>()
        .and_then(|state| state.window);
    if window.is_none() {
        let Some(created) = create_launcher_window(cx) else {
            return;
        };
        cx.global_mut::<LauncherState>().window = Some(created);
        window = Some(created);
    }

    let Some(window) = window else { return };
    if window.is_active(cx).unwrap_or(false) {
        let _ = window.update(cx, |view, _window, cx| {
            view.begin_close_transition(LauncherExitIntent::Hide);
            cx.notify();
        });
        return;
    }

    if window
        .update(cx, |view, window, cx| {
            view.font_family = style.family.clone();
            view.surface_alpha = style.surface_alpha;
            view.syntax_highlighting = style.syntax_highlighting;
            view.reset_for_show();
            window.resize(size(px(LAUNCHER_WIDTH), px(LAUNCHER_HEIGHT)));
            set_window_move_to_active_space(window);
            view.begin_open_transition();
            window.focus(&view.query_input_state.focus_handle);
            cx.notify();
            window.activate_window();
        })
        .is_err()
        && let Some(created) = create_launcher_window(cx)
    {
        cx.global_mut::<LauncherState>().window = Some(created);
        let _ = created.update(cx, |view, window, cx| {
            view.font_family = style.family.clone();
            view.surface_alpha = style.surface_alpha;
            view.syntax_highlighting = style.syntax_highlighting;
            view.reset_for_show();
            window.resize(size(px(LAUNCHER_WIDTH), px(LAUNCHER_HEIGHT)));
            set_window_move_to_active_space(window);
            view.begin_open_transition();
            window.focus(&view.query_input_state.focus_handle);
            cx.notify();
            window.activate_window();
        });
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn spawn_hotkey_listener(cx: &mut App) {
    let receiver = GlobalHotKeyEvent::receiver().clone();

    cx.spawn(async move |cx| {
        loop {
            while let Ok(event) = receiver.try_recv() {
                let is_trigger = cx
                    .try_read_global::<HotkeyRegistration, _>(|registration, _| {
                        event.id == registration.hotkey_id && event.state == HotKeyState::Pressed
                    })
                    .unwrap_or(false);

                if is_trigger {
                    let _ = cx.update(|cx| {
                        show_launcher(cx);
                    });
                }
            }

            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
        }
    })
    .detach();
}

#[cfg(target_os = "macos")]
pub(crate) fn spawn_clipboard_watcher(cx: &mut App) {
    let storage = cx.global::<StorageState>().storage.clone();

    cx.spawn(async move |cx| {
        let mut last_change_count = clipboard_change_count();
        loop {
            let _ = cx.update(|cx| {
                process_secret_autoclear(cx);
            });

            let current_change_count = clipboard_change_count();
            if current_change_count != last_change_count {
                last_change_count = current_change_count;

                if let Some(snapshot) = read_clipboard_snapshot()
                    && !snapshot.is_transient
                {
                    let should_ignore = cx
                        .update(|cx| should_ignore_self_clipboard_write(cx, &snapshot.text))
                        .unwrap_or(false);
                    if should_ignore {
                        continue;
                    }

                    let inserted = if snapshot.is_concealed {
                        storage
                            .upsert_clipboard_item_with_hint(&snapshot.text, true)
                            .unwrap_or(false)
                    } else {
                        storage
                            .upsert_clipboard_item(&snapshot.text)
                            .unwrap_or(false)
                    };
                    if inserted {
                        let _ = cx.update(|cx| {
                            if let Some(window) = cx
                                .try_global::<LauncherState>()
                                .and_then(|state| state.window)
                            {
                                let _ = window.update(cx, |view, _window, cx| {
                                    view.refresh_items();
                                    cx.notify();
                                });
                            }
                        });
                    }
                }
            }

            cx.background_executor()
                .timer(Duration::from_millis(350))
                .await;
        }
    })
    .detach();
}
