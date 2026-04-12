use crate::*;
use futures::StreamExt;

#[cfg(target_os = "macos")]
pub(crate) struct HotkeyRegistration {
    pub(crate) _manager: GlobalHotKeyManager,
    pub(crate) hotkey_id: u32,
}

#[cfg(target_os = "macos")]
impl Global for HotkeyRegistration {}

#[cfg(target_os = "macos")]
pub(crate) struct StatusItemRegistration {
    pub(crate) _status_item: StrongPtr,
    pub(crate) _menu: StrongPtr,
    pub(crate) _handler: StrongPtr,
    pub(crate) brain_on_item: StrongPtr,
    pub(crate) brain_off_item: StrongPtr,
    pub(crate) brain_download_item: StrongPtr,
}

#[cfg(target_os = "macos")]
impl Global for StatusItemRegistration {}

#[derive(Default)]
pub(crate) struct LauncherState {
    pub(crate) window: Option<WindowHandle<LauncherView>>,
}

impl Global for LauncherState {}

#[cfg(target_os = "linux")]
pub(crate) struct BackgroundAnchorState {
    pub(crate) window: Option<WindowHandle<BackgroundAnchorView>>,
}

#[cfg(target_os = "linux")]
impl Global for BackgroundAnchorState {}

#[derive(Clone)]
pub(crate) struct PendingAutoClear {
    pub(crate) due_at: Instant,
    pub(crate) expected_hash: String,
}

#[derive(Default)]
pub(crate) struct AutoClearState {
    pub(crate) pending: Option<PendingAutoClear>,
}

impl Global for AutoClearState {}

#[derive(Clone)]
pub(crate) struct PendingSelfClipboardWrite {
    pub(crate) due_at: Instant,
    pub(crate) expected_hash: String,
}

#[derive(Default)]
pub(crate) struct SelfClipboardWriteState {
    pub(crate) pending: Option<PendingSelfClipboardWrite>,
}

impl Global for SelfClipboardWriteState {}

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
                update_brain_menu_state(cx);
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
                update_brain_menu_state(cx);
            }
        }
        MenuCommand::ShowAbout => {
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("osascript")
                    .arg("-e")
                    .arg(concat!(
                        "set result to display dialog ",
                        "\"Pasta — v0.1.0\\n\\n",
                        "The clipboard manager for devs and devops.\\n",
                        "Blazing-fast, Spotlight-style clipboard launcher\\n",
                        "built with Rust and GPUI.\" ",
                        "with title \"About Pasta\" ",
                        "buttons {\"GitHub\", \"OK\"} default button 2 with icon note\n",
                        "if button returned of result is \"GitHub\" then\n",
                        "  open location \"https://github.com/yafetgetachew/pasta\"\n",
                        "end if",
                    ))
                    .spawn();
            }
            #[cfg(target_os = "linux")]
            {
                eprintln!("Pasta — v0.1.0 — The clipboard manager for devs and devops.");
                eprintln!("https://github.com/yafetgetachew/pasta");
            }
        }
        MenuCommand::SetSyntaxHighlighting(enabled) => {
            cx.global_mut::<UiStyleState>().syntax_highlighting = enabled;
            apply_style_to_open_window(cx);
            persist_ui_style_state(cx);
            update_brain_menu_state(cx);
        }
        MenuCommand::SetSecretAutoClear(enabled) => {
            cx.global_mut::<UiStyleState>().secret_auto_clear = enabled;
            persist_ui_style_state(cx);
            update_brain_menu_state(cx);
        }
        MenuCommand::SetPastaBrain(enabled) => {
            cx.global_mut::<UiStyleState>().pasta_brain_enabled = enabled;
            persist_ui_style_state(cx);
            update_brain_menu_state(cx);
        }
        MenuCommand::DownloadBrain => {
            let storage = cx.global::<StorageState>().storage.clone();
            spawn_neural_init(storage);
            update_brain_menu_state(cx);
        }
    }
}

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
                            Some(LauncherExitIntent::Hide) => {
                                #[cfg(target_os = "macos")]
                                {
                                    cx.hide();
                                }
                                #[cfg(target_os = "linux")]
                                {
                                    window.remove_window();
                                    cx.global_mut::<LauncherState>().window = None;
                                }
                            }
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
            view.pasta_brain_enabled = style.pasta_brain_enabled;
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
            view.pasta_brain_enabled = style.pasta_brain_enabled;
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

#[cfg(target_os = "linux")]
pub(crate) fn spawn_hotkey_listener(_cx: &mut App) {
    use evdev::{Device, InputEventKind, Key};
    use nix::fcntl::{FcntlArg, OFlag, fcntl};
    use std::os::fd::AsRawFd;
    use std::time::Duration;

    fn is_keyboard(device: &Device) -> bool {
        device
            .supported_keys()
            .map(|keys| keys.contains(Key::KEY_A))
            .unwrap_or(false)
    }

    fn open_keyboards() -> Vec<Device> {
        let mut keyboards = Vec::new();
        let Ok(entries) = std::fs::read_dir("/dev/input") else {
            return keyboards;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let is_event_device = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("event"))
                .unwrap_or(false);
            if !is_event_device {
                continue;
            }

            let Ok(device) = Device::open(&path) else {
                continue;
            };
            if is_keyboard(&device) {
                keyboards.push(device);
            }
        }

        keyboards
    }

    fn set_nonblocking(keyboards: &[Device]) -> Result<(), String> {
        for keyboard in keyboards {
            let fd = keyboard.as_raw_fd();
            let flags = fcntl(fd, FcntlArg::F_GETFL).map_err(|err| err.to_string())?;
            let flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
            fcntl(fd, FcntlArg::F_SETFL(flags)).map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    let Some(menu_tx) = MENU_COMMAND_TX.get().cloned() else {
        eprintln!("warning: hotkey listener unavailable: menu command channel not initialized");
        return;
    };

    std::thread::spawn(move || {
        let mut keyboards = open_keyboards();
        if keyboards.is_empty() {
            eprintln!(
                "warning: global Meta+Space hotkey unavailable: no readable keyboards in /dev/input (check input-group membership or device permissions)"
            );
            return;
        }
        if let Err(err) = set_nonblocking(&keyboards) {
            eprintln!("warning: failed to initialize Linux hotkey listener: {err}");
            return;
        }

        let mut meta_pressed = false;

        loop {
            let mut had_input_error = false;

            for keyboard in &mut keyboards {
                match keyboard.fetch_events() {
                    Ok(events) => {
                        for event in events {
                            if let InputEventKind::Key(key) = event.kind() {
                                let is_press = event.value() == 1;
                                let is_release = event.value() == 0;

                                if key == Key::KEY_LEFTMETA || key == Key::KEY_RIGHTMETA {
                                    if is_press {
                                        meta_pressed = true;
                                    } else if is_release {
                                        meta_pressed = false;
                                    }
                                    continue;
                                }

                                if key == Key::KEY_SPACE && is_press && meta_pressed {
                                    let _ = menu_tx.send(MenuCommand::ShowLauncher);
                                }
                            }
                        }
                    }
                    Err(err) => {
                        let raw = err.raw_os_error();
                        if raw != Some(nix::libc::EAGAIN) && raw != Some(nix::libc::EWOULDBLOCK) {
                            had_input_error = true;
                        }
                    }
                }
            }

            if had_input_error {
                keyboards = open_keyboards();
                if keyboards.is_empty() {
                    eprintln!(
                        "warning: Linux hotkey listener lost access to keyboard devices; stopping listener"
                    );
                    return;
                }
                if let Err(err) = set_nonblocking(&keyboards) {
                    eprintln!("warning: failed to recover Linux hotkey listener: {err}");
                    return;
                }
                meta_pressed = false;
            }

            std::thread::sleep(Duration::from_millis(12));
        }
    });
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
                                    view.refresh_items(view.preferred_refresh_execution());
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

#[cfg(target_os = "linux")]
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
                                    view.refresh_items(view.preferred_refresh_execution());
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
