#[cfg(target_os = "macos")]
use crate::*;

#[cfg(target_os = "macos")]
fn launcher_window_bounds(cx: &mut App) -> (Bounds<gpui::Pixels>, Option<gpui::DisplayId>) {
    let size = size(px(LAUNCHER_WIDTH), px(LAUNCHER_HEIGHT));

    if let Some(display) = cx.primary_display() {
        let display_bounds = display.bounds();
        let origin = point(
            display_bounds.center().x - size.center().x,
            display_bounds.origin.y + px(TOP_OFFSET),
        );

        (Bounds { origin, size }, Some(display.id()))
    } else {
        (Bounds::centered(None, size, cx), None)
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn set_window_move_to_active_space(window: &Window) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };

    unsafe {
        let ns_view: id = handle.ns_view.as_ptr().cast();
        if ns_view == nil {
            return;
        }

        let ns_window: id = msg_send![ns_view, window];
        if ns_window == nil {
            return;
        }

        let behavior: usize = msg_send![ns_window, collectionBehavior];
        let updated = behavior | NS_WINDOW_COLLECTION_BEHAVIOR_MOVE_TO_ACTIVE_SPACE;
        if updated == behavior {
            return;
        }

        let _: () = msg_send![ns_window, setCollectionBehavior: updated];
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn create_launcher_window(cx: &mut App) -> Option<WindowHandle<LauncherView>> {
    let (bounds, display_id) = launcher_window_bounds(cx);
    let storage = cx.global::<StorageState>().storage.clone();
    let style = cx.global::<UiStyleState>().clone();
    let (search_request_tx, search_result_rx, search_generation_token) =
        start_search_worker(storage.clone());

    match cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: true,
            show: false,
            kind: WindowKind::Normal,
            window_background: WindowBackgroundAppearance::Blurred,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id,
            ..Default::default()
        },
        move |window, cx| {
            let storage = storage.clone();
            let style = style.clone();
            let search_request_tx = search_request_tx.clone();
            let search_generation_token = search_generation_token.clone();
            set_window_move_to_active_space(window);
            window.on_window_should_close(cx, |_, cx| {
                cx.hide();
                false
            });

            cx.new(move |cx| {
                let view = LauncherView::new(
                    storage.clone(),
                    style.family.clone(),
                    style.surface_alpha,
                    style.syntax_highlighting,
                    style.pasta_brain_enabled,
                    search_request_tx,
                    search_generation_token,
                    cx,
                );
                cx.observe_window_activation(window, |_view: &mut LauncherView, window, cx| {
                    if window.is_window_active() {
                        _view.blur_close_armed = true;
                        return;
                    }
                    if !_view.blur_close_armed {
                        return;
                    }
                    if _view.blur_hide_suppressed() {
                        return;
                    }
                    _view.begin_close_transition(LauncherExitIntent::Hide);
                    cx.notify();
                })
                .detach();
                cx.observe_window_appearance(window, |_view: &mut LauncherView, _window, cx| {
                    cx.notify();
                })
                .detach();
                cx.observe_keystrokes(|view: &mut LauncherView, event, _window, cx| {
                    view.handle_keystroke(event, cx);
                })
                .detach();

                view
            })
        },
    ) {
        Ok(window) => {
            spawn_search_result_listener(cx, window, search_result_rx);
            Some(window)
        }
        Err(err) => {
            eprintln!("warning: failed to open launcher window: {err}");
            None
        }
    }
}
