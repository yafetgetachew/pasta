#[cfg(target_os = "macos")]
use super::state::{SearchRequest, start_search_worker};
#[cfg(target_os = "macos")]
use crate::*;
#[cfg(target_os = "macos")]
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

impl LauncherView {
    pub(crate) fn new(
        storage: Arc<ClipboardStorage>,
        font_family: SharedString,
        surface_alpha: f32,
        syntax_highlighting: bool,
    ) -> Self {
        let (search_request_tx, search_result_rx) = start_search_worker(storage.clone());
        let mut view = Self {
            storage,
            font_family,
            surface_alpha,
            syntax_highlighting,
            results_scroll: ScrollHandle::new(),
            search_request_tx,
            search_result_rx,
            next_search_request_id: 0,
            latest_search_request_id: 0,
            query: String::new(),
            query_refresh_due_at: None,
            query_select_all: false,
            items: Vec::new(),
            selected_index: 0,
            selection_changed_at: Instant::now(),
            transition_alpha: 1.0,
            transition_from: 1.0,
            transition_target: 1.0,
            transition_started_at: Instant::now(),
            transition_duration: Duration::from_millis(WINDOW_OPEN_DURATION_MS),
            pending_exit: None,
            revealed_secret_id: None,
            reveal_until: None,
            last_reveal_second_bucket: None,
            info_editor_target_id: None,
            info_editor_input: String::new(),
            tag_editor_target_id: None,
            tag_editor_input: String::new(),
            tag_editor_mode: TagEditorMode::Add,
            parameter_editor_target_id: None,
            parameter_editor_name_input: String::new(),
            parameter_editor_stage: ParameterEditorStage::SelectValue,
            parameter_editor_selected_targets: Vec::new(),
            parameter_editor_name_inputs: Vec::new(),
            parameter_editor_name_focus_index: 0,
            parameter_fill_target_id: None,
            parameter_fill_input: String::new(),
            parameter_fill_values: Vec::new(),
            parameter_fill_focus_index: 0,
            transform_menu_open: false,
            window_height: LAUNCHER_HEIGHT,
            applied_window_height: LAUNCHER_HEIGHT,
            window_height_from: LAUNCHER_HEIGHT,
            window_height_target: LAUNCHER_HEIGHT,
            window_height_started_at: Instant::now(),
            window_height_duration: Duration::from_millis(WINDOW_HEIGHT_ANIMATION_DURATION_MS),
            blur_close_armed: false,
            suppress_auto_hide: false,
            suppress_auto_hide_until: None,
            show_command_help: false,
            last_window_appearance: None,
        };
        view.request_search();
        view
    }

    pub(crate) fn reset_for_show(&mut self) {
        self.query.clear();
        self.query_refresh_due_at = None;
        self.query_select_all = false;
        self.selected_index = 0;
        self.selection_changed_at = Instant::now();
        self.items.clear();
        self.revealed_secret_id = None;
        self.reveal_until = None;
        self.last_reveal_second_bucket = None;
        self.info_editor_target_id = None;
        self.info_editor_input.clear();
        self.tag_editor_target_id = None;
        self.tag_editor_input.clear();
        self.tag_editor_mode = TagEditorMode::Add;
        self.parameter_editor_target_id = None;
        self.parameter_editor_name_input.clear();
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_fill_target_id = None;
        self.parameter_fill_input.clear();
        self.parameter_fill_values.clear();
        self.parameter_fill_focus_index = 0;
        self.transform_menu_open = false;
        self.window_height = LAUNCHER_HEIGHT;
        self.applied_window_height = LAUNCHER_HEIGHT;
        self.window_height_from = LAUNCHER_HEIGHT;
        self.window_height_target = LAUNCHER_HEIGHT;
        self.window_height_started_at = Instant::now();
        self.window_height_duration = Duration::from_millis(WINDOW_HEIGHT_ANIMATION_DURATION_MS);
        self.blur_close_armed = false;
        self.suppress_auto_hide = false;
        self.suppress_auto_hide_until = None;
        self.show_command_help = false;
        self.last_window_appearance = None;
        self.request_search();
    }

    pub(crate) fn sync_window_appearance(&mut self, window: &Window) -> bool {
        let appearance = window.appearance();
        if self.last_window_appearance == Some(appearance) {
            return false;
        }
        self.last_window_appearance = Some(appearance);
        true
    }

    pub(crate) fn begin_open_transition(&mut self) {
        self.pending_exit = None;
        self.transition_from = 0.0;
        self.transition_alpha = self.transition_from;
        self.transition_target = 1.0;
        self.transition_started_at = Instant::now();
        self.transition_duration = Duration::from_millis(WINDOW_OPEN_DURATION_MS);
    }

    pub(crate) fn begin_close_transition(&mut self, intent: LauncherExitIntent) {
        self.pending_exit = Some(intent);
        self.transition_from = self.transition_alpha.clamp(0.0, 1.0);
        self.transition_target = 0.0;
        self.transition_started_at = Instant::now();
        self.transition_duration = Duration::from_millis(WINDOW_CLOSE_DURATION_MS);
    }

    pub(crate) fn transition_running(&self) -> bool {
        (self.transition_alpha - self.transition_target).abs() > 0.001
            || (self.transition_target == 0.0 && self.pending_exit.is_some())
    }

    pub(crate) fn clear_expired_secret_reveal(&mut self) -> bool {
        if let Some(until) = self.reveal_until
            && Instant::now() >= until
        {
            self.revealed_secret_id = None;
            self.reveal_until = None;
            self.last_reveal_second_bucket = None;
            return true;
        }

        false
    }

    pub(crate) fn secret_seconds_left(&self, item_id: i64) -> Option<u64> {
        if self.revealed_secret_id != Some(item_id) {
            return None;
        }
        let until = self.reveal_until?;
        let now = Instant::now();
        if until <= now {
            return None;
        }

        Some((until - now).as_secs().saturating_add(1))
    }

    pub(crate) fn is_secret_masked(&self, item_id: i64) -> bool {
        self.secret_seconds_left(item_id).is_none()
    }

    pub(crate) fn reveal_and_copy_selected_secret(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(self.selected_index).cloned() else {
            return;
        };
        if item.item_type != ClipboardItemType::Password {
            return;
        }

        if !self.can_copy_secret_now(item.id) {
            self.reveal_secret(item.id, true, cx);
            return;
        }

        self.copy_selected_to_clipboard(cx);
    }

    pub(crate) fn reveal_secret(&mut self, item_id: i64, copy_after: bool, cx: &mut Context<Self>) {
        let Some(item) = self.items.iter().find(|item| item.id == item_id).cloned() else {
            return;
        };
        if item.item_type != ClipboardItemType::Password {
            return;
        }
        self.suppress_auto_hide = true;
        let authenticated = authenticate_with_touch_id("Reveal secret in Pasta");
        self.suppress_auto_hide = false;
        self.suppress_auto_hide_until = Some(Instant::now() + Duration::from_millis(250));
        if !authenticated {
            return;
        }

        cx.activate(true);
        self.revealed_secret_id = Some(item.id);
        self.reveal_until = Some(Instant::now() + Duration::from_secs(12));

        if copy_after && let Some(ix) = self.items.iter().position(|i| i.id == item.id) {
            self.copy_index_to_clipboard(ix, cx);
            return;
        }

        show_macos_notification("Pasta", "Secret revealed. Press Enter again to copy.");
        cx.notify();
    }

    pub(crate) fn can_copy_secret_now(&self, item_id: i64) -> bool {
        !self.is_secret_masked(item_id) && self.revealed_secret_id == Some(item_id)
    }

    pub(crate) fn blur_hide_suppressed(&mut self) -> bool {
        if self.suppress_auto_hide {
            return true;
        }
        if let Some(until) = self.suppress_auto_hide_until {
            if Instant::now() < until {
                return true;
            }
            self.suppress_auto_hide_until = None;
        }
        false
    }

    pub(crate) fn schedule_secret_autoclear(&self, content: &str, cx: &mut Context<Self>) {
        if !cx.global::<UiStyleState>().secret_auto_clear {
            return;
        }

        cx.global_mut::<AutoClearState>().pending = Some(PendingAutoClear {
            due_at: Instant::now() + Duration::from_secs(30),
            expected_hash: clipboard_text_hash(content),
        });
    }

    pub(crate) fn mark_self_clipboard_write(&self, content: &str, cx: &mut Context<Self>) {
        cx.global_mut::<SelfClipboardWriteState>().pending = Some(PendingSelfClipboardWrite {
            due_at: Instant::now() + Duration::from_secs(5),
            expected_hash: clipboard_text_hash(content),
        });
    }

    pub(crate) fn tick_transition(&mut self) -> Option<LauncherExitIntent> {
        let duration_secs = self.transition_duration.as_secs_f32().max(0.001);
        let elapsed_secs = (Instant::now() - self.transition_started_at).as_secs_f32();
        let t = (elapsed_secs / duration_secs).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        self.transition_alpha =
            self.transition_from + (self.transition_target - self.transition_from) * eased;

        if t >= 1.0 {
            self.transition_alpha = self.transition_target;
            if self.transition_target <= 0.0 && self.pending_exit.is_some() {
                return self.pending_exit.take();
            }
        }

        if self.transition_target <= 0.0
            && self.transition_alpha <= WINDOW_CLOSE_EARLY_EXIT_ALPHA
            && self.pending_exit.is_some()
        {
            self.transition_alpha = 0.0;
            return self.pending_exit.take();
        }

        None
    }

    pub(crate) fn tick_window_height_animation(&mut self, window: &mut Window) -> bool {
        let mut animating =
            (self.window_height_target - self.window_height).abs() > WINDOW_HEIGHT_ANIMATION_SNAP;
        if animating {
            let duration_secs = self.window_height_duration.as_secs_f32().max(0.001);
            let elapsed_secs = (Instant::now() - self.window_height_started_at).as_secs_f32();
            let t = (elapsed_secs / duration_secs).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - t).powi(3);
            self.window_height = (self.window_height_from
                + (self.window_height_target - self.window_height_from) * eased)
                .clamp(LAUNCHER_HEIGHT, LAUNCHER_EXPANDED_HEIGHT);

            if t >= 1.0
                || (self.window_height_target - self.window_height).abs()
                    <= WINDOW_HEIGHT_ANIMATION_SNAP
            {
                self.window_height = self.window_height_target;
                animating = false;
            }
        }

        let quantized_height =
            (self.window_height / WINDOW_HEIGHT_RESIZE_STEP).round() * WINDOW_HEIGHT_RESIZE_STEP;
        self.window_height = quantized_height.clamp(LAUNCHER_HEIGHT, LAUNCHER_EXPANDED_HEIGHT);

        let needs_resize =
            (self.window_height - self.applied_window_height).abs() >= WINDOW_HEIGHT_RESIZE_STEP;
        if needs_resize {
            window.resize(size(px(LAUNCHER_WIDTH), px(self.window_height)));
            self.applied_window_height = self.window_height;
        }

        animating || needs_resize
    }

    pub(crate) fn secret_countdown_tick_changed(&mut self) -> bool {
        let Some(until) = self.reveal_until else {
            return self.last_reveal_second_bucket.take().is_some();
        };
        let now = Instant::now();
        if until <= now {
            return false;
        }
        let bucket = (until - now).as_secs();
        if self.last_reveal_second_bucket == Some(bucket) {
            return false;
        }
        self.last_reveal_second_bucket = Some(bucket);
        true
    }

    pub(crate) fn refresh_items(&mut self) {
        self.items = self
            .storage
            .search_items(&self.query, 48)
            .unwrap_or_else(|_| Vec::new());
        if self.selected_index >= self.items.len() {
            self.selected_index = 0;
        }
    }

    pub(crate) fn request_search(&mut self) {
        self.next_search_request_id = self.next_search_request_id.wrapping_add(1);
        let request_id = self.next_search_request_id;
        self.latest_search_request_id = request_id;

        if self
            .search_request_tx
            .send(SearchRequest {
                request_id,
                query: self.query.clone(),
            })
            .is_err()
        {
            // Fallback for environments where the worker thread is unavailable.
            self.refresh_items();
        }
    }

    pub(crate) fn drain_search_results(&mut self) -> bool {
        let mut changed = false;
        while let Ok(response) = self.search_result_rx.try_recv() {
            if response.request_id < self.latest_search_request_id {
                continue;
            }

            self.items = response.items;
            if self.selected_index >= self.items.len() {
                self.selected_index = 0;
            }
            changed = true;
        }
        changed
    }

    pub(crate) fn schedule_query_refresh(&mut self) {
        self.query_refresh_due_at =
            Some(Instant::now() + Duration::from_millis(QUERY_REFRESH_DEBOUNCE_MS));
    }

    pub(crate) fn flush_pending_query_refresh(&mut self) -> bool {
        if self.query_refresh_due_at.take().is_none() {
            return false;
        }
        self.request_search();
        true
    }

    pub(crate) fn tick_query_refresh(&mut self) -> bool {
        let Some(due_at) = self.query_refresh_due_at else {
            return false;
        };
        if Instant::now() < due_at {
            return false;
        }
        self.flush_pending_query_refresh()
    }

    pub(crate) fn filter_visible_items_for_query(&mut self, query: &str) {
        let normalized = query.trim().to_lowercase();
        if normalized.is_empty() {
            return;
        }

        let tag_only = normalized.starts_with('/');
        let effective_query = if tag_only {
            normalized.trim_start_matches('/').trim().to_owned()
        } else {
            normalized
        };
        if effective_query.is_empty() {
            return;
        }

        self.items
            .retain(|record| record_matches_query(record, &effective_query, tag_only));
        if self.selected_index >= self.items.len() {
            self.selected_index = 0;
        }
    }

    pub(crate) fn move_selection(&mut self, direction: i32, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            self.selected_index = 0;
            return;
        }

        let previous_index = self.selected_index;
        if direction > 0 {
            if self.selected_index + 1 < self.items.len() {
                self.selected_index += 1;
            }
        } else if direction < 0 {
            self.selected_index = self.selected_index.saturating_sub(1);
        }

        if self.selected_index != previous_index {
            self.selection_changed_at = Instant::now();
            self.results_scroll.scroll_to_item(self.selected_index);
            cx.notify();
        }
    }

    pub(crate) fn copy_selected_to_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(self.selected_index).cloned() else {
            return;
        };

        if item.item_type == ClipboardItemType::Password && !self.can_copy_secret_now(item.id) {
            self.reveal_secret(item.id, true, cx);
            return;
        }

        if !item.parameters.is_empty() {
            self.open_parameter_fill_prompt(item.id, &item.parameters, cx);
            return;
        }

        self.mark_self_clipboard_write(&item.content, cx);
        cx.write_to_clipboard(ClipboardItem::new_string(item.content.clone()));
        if item.item_type == ClipboardItemType::Password {
            self.schedule_secret_autoclear(&item.content, cx);
            self.revealed_secret_id = Some(item.id);
            self.reveal_until = Some(Instant::now() + Duration::from_secs(12));
            let body = if cx.global::<UiStyleState>().secret_auto_clear {
                "Secret copied to clipboard. Auto-clear in 30 seconds."
            } else {
                "Secret copied to clipboard."
            };
            show_macos_notification("Pasta", body);
            cx.notify();
            return;
        } else {
            show_macos_notification("Pasta", "Copied to clipboard.");
        }
        self.begin_close_transition(LauncherExitIntent::Hide);
        cx.notify();
    }

    pub(crate) fn copy_index_to_clipboard(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected_index = index;
        self.selection_changed_at = Instant::now();
        self.results_scroll.scroll_to_item(self.selected_index);
        self.copy_selected_to_clipboard(cx);
    }

    pub(crate) fn delete_selected_item(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.items.get(self.selected_index).map(|item| item.id) else {
            return;
        };

        match self.storage.delete_item(item_id) {
            Ok(_) => {
                self.refresh_items();
                if !self.items.is_empty() {
                    self.results_scroll.scroll_to_item(self.selected_index);
                }
                self.selection_changed_at = Instant::now();
                cx.notify();
            }
            Err(err) => {
                eprintln!("warning: failed to delete clipboard item: {err}");
            }
        }
    }

    pub(crate) fn mark_selected_item_as_secret(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.items.get(self.selected_index).map(|item| item.id) else {
            return;
        };

        match self.storage.mark_item_as_secret(item_id) {
            Ok(true) => {
                self.revealed_secret_id = None;
                self.reveal_until = None;
                self.last_reveal_second_bucket = None;

                let previous_index = self.selected_index;
                self.refresh_items();
                if let Some(ix) = self.items.iter().position(|entry| entry.id == item_id) {
                    self.selected_index = ix;
                } else if !self.items.is_empty() {
                    self.selected_index = previous_index.min(self.items.len().saturating_sub(1));
                } else {
                    self.selected_index = 0;
                }
                if !self.items.is_empty() {
                    self.results_scroll.scroll_to_item(self.selected_index);
                }
                self.selection_changed_at = Instant::now();
                show_macos_notification("Pasta", "Item marked as secret.");
                cx.notify();
            }
            Ok(false) => {
                show_macos_notification("Pasta", "Item is already protected.");
            }
            Err(err) => {
                eprintln!("warning: failed to mark item as secret: {err}");
                show_macos_notification("Pasta", "Failed to mark item as secret.");
            }
        }
    }

    pub(crate) fn update_query(&mut self, query: String, cx: &mut Context<Self>) {
        let previous_query = self.query.clone();
        self.query = query;
        self.query_select_all = false;
        self.selected_index = 0;
        self.selection_changed_at = Instant::now();
        if !previous_query.is_empty() && self.query.starts_with(&previous_query) {
            let query = self.query.clone();
            self.filter_visible_items_for_query(&query);
        }
        self.schedule_query_refresh();
        cx.notify();
    }

    pub(crate) fn start_info_editor_for_selected(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(self.selected_index).cloned() else {
            return;
        };

        self.info_editor_target_id = Some(item.id);
        self.info_editor_input = item.description;
        self.tag_editor_target_id = None;
        self.tag_editor_input.clear();
        self.tag_editor_mode = TagEditorMode::Add;
        self.parameter_editor_target_id = None;
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_input.clear();
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        self.parameter_fill_target_id = None;
        self.parameter_fill_input.clear();
        self.parameter_fill_values.clear();
        self.parameter_fill_focus_index = 0;
        self.transform_menu_open = false;
        cx.notify();
    }

    pub(crate) fn commit_info_editor(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.info_editor_target_id else {
            return;
        };
        let normalized = self.info_editor_input.trim().to_owned();
        match self.storage.upsert_item_description(item_id, &normalized) {
            Ok(true) => {
                let previous_index = self.selected_index;
                self.refresh_items();
                if let Some(ix) = self.items.iter().position(|entry| entry.id == item_id) {
                    self.selected_index = ix;
                } else if !self.items.is_empty() {
                    self.selected_index = previous_index.min(self.items.len().saturating_sub(1));
                } else {
                    self.selected_index = 0;
                }
                if !self.items.is_empty() {
                    self.results_scroll.scroll_to_item(self.selected_index);
                }
                self.selection_changed_at = Instant::now();
                self.info_editor_target_id = None;
                self.info_editor_input.clear();
                show_macos_notification(
                    "Pasta",
                    if normalized.is_empty() {
                        "Info cleared."
                    } else {
                        "Info saved."
                    },
                );
                cx.notify();
            }
            Ok(false) => {
                self.info_editor_target_id = None;
                self.info_editor_input.clear();
                show_macos_notification("Pasta", "Info unchanged.");
                cx.notify();
            }
            Err(err) => {
                eprintln!("warning: failed to update snippet info: {err}");
                show_macos_notification("Pasta", "Failed to save info.");
            }
        }
    }

    pub(crate) fn cancel_info_editor(&mut self, cx: &mut Context<Self>) {
        self.info_editor_target_id = None;
        self.info_editor_input.clear();
        cx.notify();
    }

    pub(crate) fn add_custom_tags_to_selected(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.items.get(self.selected_index).map(|item| item.id) else {
            return;
        };
        self.info_editor_target_id = None;
        self.info_editor_input.clear();
        self.tag_editor_mode = TagEditorMode::Add;
        self.tag_editor_target_id = Some(item_id);
        self.tag_editor_input.clear();
        cx.notify();
    }

    pub(crate) fn remove_custom_tags_from_selected(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.items.get(self.selected_index).map(|item| item.id) else {
            return;
        };
        self.info_editor_target_id = None;
        self.info_editor_input.clear();
        self.tag_editor_mode = TagEditorMode::Remove;
        self.tag_editor_target_id = Some(item_id);
        self.tag_editor_input.clear();
        cx.notify();
    }

    pub(crate) fn commit_custom_tags(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.tag_editor_target_id else {
            return;
        };
        let tags = parse_custom_tags_input(&self.tag_editor_input);
        if tags.is_empty() {
            show_macos_notification("Pasta", "No valid tags entered.");
            return;
        }

        let result = match self.tag_editor_mode {
            TagEditorMode::Add => self.storage.add_custom_tags(item_id, &tags),
            TagEditorMode::Remove => self.storage.remove_custom_tags(item_id, &tags),
        };

        match result {
            Ok(true) => {
                let previous_index = self.selected_index;
                self.refresh_items();
                if let Some(ix) = self.items.iter().position(|entry| entry.id == item_id) {
                    self.selected_index = ix;
                    self.selection_changed_at = Instant::now();
                    self.results_scroll.scroll_to_item(ix);
                } else if !self.items.is_empty() {
                    self.selected_index = previous_index.min(self.items.len().saturating_sub(1));
                    self.selection_changed_at = Instant::now();
                    self.results_scroll.scroll_to_item(self.selected_index);
                }

                self.tag_editor_target_id = None;
                self.tag_editor_input.clear();
                show_macos_notification(
                    "Pasta",
                    if self.tag_editor_mode == TagEditorMode::Add {
                        "Custom tags saved."
                    } else {
                        "Tags removed."
                    },
                );
                self.tag_editor_mode = TagEditorMode::Add;
                cx.notify();
            }
            Ok(false) => {
                self.tag_editor_target_id = None;
                self.tag_editor_input.clear();
                show_macos_notification(
                    "Pasta",
                    if self.tag_editor_mode == TagEditorMode::Add {
                        "No new tags were added."
                    } else {
                        "No matching removable tags."
                    },
                );
                self.tag_editor_mode = TagEditorMode::Add;
                cx.notify();
            }
            Err(err) => {
                eprintln!("warning: failed to update custom tags: {err}");
                show_macos_notification("Pasta", "Failed to update tags.");
            }
        }
    }

    pub(crate) fn cancel_custom_tags_editor(&mut self, cx: &mut Context<Self>) {
        self.tag_editor_target_id = None;
        self.tag_editor_input.clear();
        self.tag_editor_mode = TagEditorMode::Add;
        cx.notify();
    }

    pub(crate) fn start_parameter_editor_for_selected(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(self.selected_index).cloned() else {
            return;
        };
        if item.item_type == ClipboardItemType::Password {
            show_macos_notification("Pasta", "Secrets cannot be parametrized.");
            return;
        }

        self.info_editor_target_id = None;
        self.info_editor_input.clear();
        self.parameter_editor_target_id = Some(item.id);
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_input.clear();
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        self.parameter_fill_target_id = None;
        self.parameter_fill_input.clear();
        self.parameter_fill_values.clear();
        self.parameter_fill_focus_index = 0;
        self.transform_menu_open = false;
        self.tag_editor_target_id = None;
        cx.notify();
    }

    pub(crate) fn cancel_parameter_editor(&mut self, cx: &mut Context<Self>) {
        self.parameter_editor_target_id = None;
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_input.clear();
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        cx.notify();
    }

    fn sync_parameter_editor_name_shadow(&mut self) {
        self.parameter_editor_name_input = self
            .parameter_editor_name_inputs
            .get(self.parameter_editor_name_focus_index)
            .cloned()
            .unwrap_or_default();
    }

    fn sync_parameter_editor_name_inputs(&mut self) {
        let mut existing = HashMap::new();
        for (target, name) in self
            .parameter_editor_selected_targets
            .iter()
            .cloned()
            .zip(self.parameter_editor_name_inputs.iter().cloned())
        {
            existing.insert(target, name);
        }

        self.parameter_editor_name_inputs = self
            .parameter_editor_selected_targets
            .iter()
            .map(|target| existing.remove(target).unwrap_or_default())
            .collect();

        if self.parameter_editor_name_focus_index >= self.parameter_editor_name_inputs.len() {
            self.parameter_editor_name_focus_index =
                self.parameter_editor_name_inputs.len().saturating_sub(1);
        }
        self.sync_parameter_editor_name_shadow();
    }

    fn set_parameter_target(&mut self, target: &str) {
        let normalized = target.trim();
        if normalized.is_empty() {
            return;
        }
        self.parameter_editor_selected_targets = vec![normalized.to_owned()];
        self.parameter_editor_name_focus_index = 0;
        self.sync_parameter_editor_name_inputs();
    }

    fn toggle_parameter_target(&mut self, target: &str) {
        let normalized = target.trim();
        if normalized.is_empty() {
            return;
        }
        if let Some(ix) = self
            .parameter_editor_selected_targets
            .iter()
            .position(|existing| existing == normalized)
        {
            self.parameter_editor_selected_targets.remove(ix);
        } else {
            self.parameter_editor_selected_targets
                .push(normalized.to_owned());
        }
        self.parameter_editor_name_focus_index = self
            .parameter_editor_selected_targets
            .len()
            .saturating_sub(1);
        self.sync_parameter_editor_name_inputs();
    }

    fn ensure_parameter_editor_target_added(&self) -> bool {
        !self.parameter_editor_selected_targets.is_empty()
    }

    fn focus_next_parameter_name_input(&mut self) {
        if self.parameter_editor_name_inputs.is_empty() {
            self.parameter_editor_name_focus_index = 0;
        } else {
            self.parameter_editor_name_focus_index = (self.parameter_editor_name_focus_index + 1)
                % self.parameter_editor_name_inputs.len();
        }
        self.sync_parameter_editor_name_shadow();
    }

    fn focus_previous_parameter_name_input(&mut self) {
        if self.parameter_editor_name_inputs.is_empty() {
            self.parameter_editor_name_focus_index = 0;
        } else if self.parameter_editor_name_focus_index == 0 {
            self.parameter_editor_name_focus_index =
                self.parameter_editor_name_inputs.len().saturating_sub(1);
        } else {
            self.parameter_editor_name_focus_index -= 1;
        }
        self.sync_parameter_editor_name_shadow();
    }

    pub(crate) fn focus_parameter_name_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.parameter_editor_name_inputs.is_empty() {
            return;
        }
        self.parameter_editor_name_focus_index =
            index.min(self.parameter_editor_name_inputs.len() - 1);
        self.sync_parameter_editor_name_shadow();
        cx.notify();
    }

    fn active_parameter_name_input_mut(&mut self) -> Option<&mut String> {
        if self.parameter_editor_name_inputs.is_empty() {
            return None;
        }
        let max_ix = self.parameter_editor_name_inputs.len().saturating_sub(1);
        if self.parameter_editor_name_focus_index > max_ix {
            self.parameter_editor_name_focus_index = max_ix;
        }
        self.parameter_editor_name_inputs
            .get_mut(self.parameter_editor_name_focus_index)
    }

    pub(crate) fn commit_parameter_editor(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.parameter_editor_target_id else {
            return;
        };
        if self.parameter_editor_selected_targets.is_empty() {
            show_macos_notification("Pasta", "Select one or more token buttons first.");
            return;
        }

        if self.parameter_editor_name_inputs.len() != self.parameter_editor_selected_targets.len() {
            self.sync_parameter_editor_name_inputs();
        }

        if self.parameter_editor_name_inputs.len() != self.parameter_editor_selected_targets.len() {
            show_macos_notification("Pasta", "Parameter naming state is invalid.");
            return;
        }

        let mut seen_names = HashSet::new();
        for (ix, name) in self.parameter_editor_name_inputs.iter().enumerate() {
            let trimmed = name.trim();
            if !is_valid_parameter_name(trimmed) {
                self.parameter_editor_name_focus_index = ix;
                self.sync_parameter_editor_name_shadow();
                show_macos_notification(
                    "Pasta",
                    "Each parameter name must start with a letter/underscore and use letters, numbers, or underscores.",
                );
                cx.notify();
                return;
            }
            if !seen_names.insert(trimmed.to_ascii_lowercase()) {
                self.parameter_editor_name_focus_index = ix;
                self.sync_parameter_editor_name_shadow();
                show_macos_notification("Pasta", "Parameter names must be unique.");
                cx.notify();
                return;
            }
        }

        let mut changed = false;
        for (name, target) in self
            .parameter_editor_name_inputs
            .iter()
            .zip(self.parameter_editor_selected_targets.iter())
        {
            match self
                .storage
                .upsert_item_parameter(item_id, name.trim(), target.trim())
            {
                Ok(updated) => changed |= updated,
                Err(err) => {
                    eprintln!("warning: failed to save parameter: {err}");
                    show_macos_notification("Pasta", "Failed to save parameter.");
                    return;
                }
            }
        }

        if changed {
            let previous_index = self.selected_index;
            self.refresh_items();
            if let Some(ix) = self.items.iter().position(|entry| entry.id == item_id) {
                self.selected_index = ix;
            } else if !self.items.is_empty() {
                self.selected_index = previous_index.min(self.items.len().saturating_sub(1));
            } else {
                self.selected_index = 0;
            }
            if !self.items.is_empty() {
                self.results_scroll.scroll_to_item(self.selected_index);
            }
            self.selection_changed_at = Instant::now();
            self.parameter_editor_target_id = None;
            self.parameter_editor_selected_targets.clear();
            self.parameter_editor_name_inputs.clear();
            self.parameter_editor_name_focus_index = 0;
            self.parameter_editor_name_input.clear();
            self.parameter_editor_stage = ParameterEditorStage::SelectValue;
            show_macos_notification("Pasta", "Parameters saved.");
            cx.notify();
        } else {
            show_macos_notification("Pasta", "No parameter changes were applied.");
        }
    }

    pub(crate) fn select_parameter_clickable_range(
        &mut self,
        range_index: usize,
        additive: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(item_id) = self.parameter_editor_target_id else {
            return;
        };
        let Some(content) = self
            .items
            .iter()
            .find(|entry| entry.id == item_id)
            .map(|entry| entry.content.clone())
        else {
            return;
        };
        let ranges = parameter_clickable_ranges(&content);
        let Some(range) = ranges.get(range_index) else {
            return;
        };
        let Some(target) = content.get(range.clone()) else {
            return;
        };

        if additive {
            self.toggle_parameter_target(target);
        } else {
            self.set_parameter_target(target);
        }
        cx.notify();
    }

    pub(crate) fn handle_parameter_editor_keystroke(
        &mut self,
        event: &KeystrokeEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        let no_modifiers = !modifiers.modified();
        let platform_only =
            modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.function;
        let shift_only = modifiers.shift && !modifiers.platform && !modifiers.control;
        if self.parameter_editor_target_id.is_none() {
            return;
        }

        if self.parameter_editor_stage == ParameterEditorStage::EnterName {
            if self.parameter_editor_name_inputs.is_empty() {
                self.sync_parameter_editor_name_inputs();
            }
            match key {
                "escape" | "esc" => {
                    self.cancel_parameter_editor(cx);
                    return;
                }
                "tab" if no_modifiers => {
                    self.focus_next_parameter_name_input();
                    cx.notify();
                    return;
                }
                "tab" if shift_only => {
                    self.focus_previous_parameter_name_input();
                    cx.notify();
                    return;
                }
                "enter" | "return" => {
                    self.commit_parameter_editor(cx);
                    return;
                }
                "backspace" if no_modifiers => {
                    if let Some(active) = self.active_parameter_name_input_mut() {
                        active.pop();
                        self.sync_parameter_editor_name_shadow();
                    }
                    cx.notify();
                    return;
                }
                "v" if platform_only => {
                    if let Some(text) = read_clipboard_text()
                        && let Some(active) = self.active_parameter_name_input_mut()
                    {
                        active.push_str(text.trim());
                        self.sync_parameter_editor_name_shadow();
                        cx.notify();
                    }
                    return;
                }
                "up" | "arrowup" => {
                    self.focus_previous_parameter_name_input();
                    cx.notify();
                    return;
                }
                "down" | "arrowdown" => {
                    self.focus_next_parameter_name_input();
                    cx.notify();
                    return;
                }
                _ => {}
            }

            if let Some(character) = typed_character(event)
                && let Some(active) = self.active_parameter_name_input_mut()
            {
                active.push(character);
                self.sync_parameter_editor_name_shadow();
                cx.notify();
            }
            return;
        }

        match key {
            "escape" | "esc" => {
                self.cancel_parameter_editor(cx);
            }
            "tab" if no_modifiers || shift_only => {
                if !self.ensure_parameter_editor_target_added() {
                    show_macos_notification("Pasta", "Select one or more token buttons first.");
                    return;
                }
                self.parameter_editor_stage = ParameterEditorStage::EnterName;
                self.sync_parameter_editor_name_inputs();
                cx.notify();
            }
            "enter" | "return" => {
                if !self.ensure_parameter_editor_target_added() {
                    show_macos_notification("Pasta", "Select one or more token buttons first.");
                    return;
                }
                self.parameter_editor_stage = ParameterEditorStage::EnterName;
                self.sync_parameter_editor_name_inputs();
                cx.notify();
            }
            "p" if no_modifiers => {
                if !self.ensure_parameter_editor_target_added() {
                    show_macos_notification("Pasta", "Select one or more token buttons first.");
                    return;
                }
                self.parameter_editor_stage = ParameterEditorStage::EnterName;
                self.sync_parameter_editor_name_inputs();
                cx.notify();
            }
            _ => {}
        }
    }

    pub(crate) fn open_parameter_fill_prompt(
        &mut self,
        item_id: i64,
        parameters: &[ClipboardParameter],
        cx: &mut Context<Self>,
    ) {
        self.info_editor_target_id = None;
        self.info_editor_input.clear();
        self.parameter_fill_target_id = Some(item_id);
        self.parameter_fill_values = vec![String::new(); parameters.len()];
        self.parameter_fill_focus_index = 0;
        self.parameter_fill_input.clear();
        self.parameter_editor_target_id = None;
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_input.clear();
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        self.transform_menu_open = false;
        self.tag_editor_target_id = None;
        cx.notify();
    }

    pub(crate) fn cancel_parameter_fill_prompt(&mut self, cx: &mut Context<Self>) {
        self.parameter_fill_target_id = None;
        self.parameter_fill_input.clear();
        self.parameter_fill_values.clear();
        self.parameter_fill_focus_index = 0;
        cx.notify();
    }

    pub(crate) fn focus_parameter_fill_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.parameter_fill_values.is_empty() {
            return;
        }
        self.parameter_fill_focus_index = index.min(self.parameter_fill_values.len() - 1);
        cx.notify();
    }

    pub(crate) fn commit_parameter_fill_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.parameter_fill_target_id else {
            return;
        };
        let Some(item) = self.items.iter().find(|entry| entry.id == item_id).cloned() else {
            self.parameter_fill_target_id = None;
            self.parameter_fill_input.clear();
            self.parameter_fill_values.clear();
            self.parameter_fill_focus_index = 0;
            cx.notify();
            return;
        };

        if self.parameter_fill_values.len() != item.parameters.len() {
            self.parameter_fill_values = vec![String::new(); item.parameters.len()];
            self.parameter_fill_focus_index = 0;
        }

        let trimmed_values: Vec<String> = self
            .parameter_fill_values
            .iter()
            .map(|value| value.trim().to_owned())
            .collect();
        let all_blank = trimmed_values.iter().all(|value| value.is_empty());
        let rendered = if all_blank {
            item.content.clone()
        } else {
            if trimmed_values.iter().any(|value| value.is_empty()) {
                show_macos_notification(
                    "Pasta",
                    "Fill all parameter fields, or leave all blank to copy original.",
                );
                return;
            }

            let assignments: HashMap<String, String> = item
                .parameters
                .iter()
                .zip(trimmed_values.iter())
                .map(|(parameter, value)| (parameter.name.clone(), value.clone()))
                .collect();
            match render_parameterized_content(&item.content, &item.parameters, &assignments) {
                Ok(rendered) => rendered,
                Err(err) => {
                    show_macos_notification("Pasta", &format!("Parameter fill failed: {err}"));
                    return;
                }
            }
        };

        self.mark_self_clipboard_write(&rendered, cx);
        cx.write_to_clipboard(ClipboardItem::new_string(rendered));
        self.parameter_fill_target_id = None;
        self.parameter_fill_input.clear();
        self.parameter_fill_values.clear();
        self.parameter_fill_focus_index = 0;
        show_macos_notification(
            "Pasta",
            if all_blank {
                "Copied original snippet."
            } else {
                "Copied with parameters."
            },
        );
        self.begin_close_transition(LauncherExitIntent::Hide);
        cx.notify();
    }

    pub(crate) fn handle_parameter_fill_keystroke(
        &mut self,
        event: &KeystrokeEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        let no_modifiers = !modifiers.modified();
        let platform_only =
            modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.function;
        let shift_only = modifiers.shift && !modifiers.platform && !modifiers.control;

        if self.parameter_fill_values.is_empty() {
            self.parameter_fill_values.push(String::new());
            self.parameter_fill_focus_index = 0;
        }

        match key {
            "escape" | "esc" => {
                self.cancel_parameter_fill_prompt(cx);
                return;
            }
            "tab" if no_modifiers => {
                self.parameter_fill_focus_index =
                    (self.parameter_fill_focus_index + 1) % self.parameter_fill_values.len();
                cx.notify();
                return;
            }
            "tab" if shift_only => {
                if self.parameter_fill_focus_index == 0 {
                    self.parameter_fill_focus_index = self.parameter_fill_values.len() - 1;
                } else {
                    self.parameter_fill_focus_index -= 1;
                }
                cx.notify();
                return;
            }
            "up" | "arrowup" => {
                if self.parameter_fill_focus_index == 0 {
                    self.parameter_fill_focus_index = self.parameter_fill_values.len() - 1;
                } else {
                    self.parameter_fill_focus_index -= 1;
                }
                cx.notify();
                return;
            }
            "down" | "arrowdown" => {
                self.parameter_fill_focus_index =
                    (self.parameter_fill_focus_index + 1) % self.parameter_fill_values.len();
                cx.notify();
                return;
            }
            "enter" | "return" => {
                self.commit_parameter_fill_prompt(cx);
                return;
            }
            "backspace" if no_modifiers => {
                if let Some(active) = self
                    .parameter_fill_values
                    .get_mut(self.parameter_fill_focus_index)
                {
                    active.pop();
                }
                cx.notify();
                return;
            }
            "v" if platform_only => {
                if let Some(text) = read_clipboard_text() {
                    if let Some(active) = self
                        .parameter_fill_values
                        .get_mut(self.parameter_fill_focus_index)
                    {
                        active.push_str(text.trim());
                    }
                    cx.notify();
                }
                return;
            }
            _ => {}
        }

        if let Some(character) = typed_character(event) {
            if let Some(active) = self
                .parameter_fill_values
                .get_mut(self.parameter_fill_focus_index)
            {
                active.push(character);
            }
            cx.notify();
        }
    }

    pub(crate) fn handle_info_editor_keystroke(
        &mut self,
        event: &KeystrokeEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        let no_modifiers = !modifiers.modified();
        let platform_only =
            modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.function;

        match key {
            "escape" | "esc" => {
                self.cancel_info_editor(cx);
                return;
            }
            "enter" | "return" => {
                self.commit_info_editor(cx);
                return;
            }
            "backspace" if no_modifiers => {
                self.info_editor_input.pop();
                cx.notify();
                return;
            }
            "v" if platform_only => {
                if let Some(text) = read_clipboard_text() {
                    self.info_editor_input.push_str(&text);
                    cx.notify();
                }
                return;
            }
            _ => {}
        }

        if let Some(character) = typed_character(event) {
            self.info_editor_input.push(character);
            cx.notify();
        }
    }

    pub(crate) fn handle_tag_editor_keystroke(
        &mut self,
        event: &KeystrokeEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        let no_modifiers = !modifiers.modified();
        let platform_only =
            modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.function;

        match key {
            "escape" | "esc" => {
                self.cancel_custom_tags_editor(cx);
                return;
            }
            "enter" | "return" => {
                self.commit_custom_tags(cx);
                return;
            }
            "backspace" if no_modifiers => {
                self.tag_editor_input.pop();
                cx.notify();
                return;
            }
            "v" if platform_only => {
                if let Some(text) = read_clipboard_text() {
                    self.tag_editor_input.push_str(&text);
                    cx.notify();
                }
                return;
            }
            _ => {}
        }

        if let Some(character) = typed_character(event) {
            self.tag_editor_input.push(character);
            cx.notify();
        }
    }

    pub(crate) fn toggle_transform_menu(&mut self, cx: &mut Context<Self>) {
        if self.items.get(self.selected_index).is_none() {
            show_macos_notification("Pasta", "No item selected to transform.");
            return;
        }
        self.transform_menu_open = !self.transform_menu_open;
        if self.transform_menu_open {
            self.show_command_help = false;
        }
        cx.notify();
    }

    pub(crate) fn handle_transform_keystroke(
        &mut self,
        event: &KeystrokeEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        let has_disallowed_modifiers =
            modifiers.control || modifiers.alt || modifiers.platform || modifiers.function;

        if key == "escape" || key == "esc" || (key == "tab" && !has_disallowed_modifiers) {
            self.transform_menu_open = false;
            cx.notify();
            return;
        }

        if has_disallowed_modifiers {
            return;
        }

        let action = transform_action_for_shortcut(key, modifiers);

        if let Some(action) = action {
            self.apply_transform_action(action, cx);
        }
    }

    pub(crate) fn apply_transform_action(
        &mut self,
        action: TransformAction,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.items.get(self.selected_index).cloned() else {
            self.transform_menu_open = false;
            cx.notify();
            return;
        };

        if item.item_type == ClipboardItemType::Password && !self.can_copy_secret_now(item.id) {
            show_macos_notification("Pasta", "Reveal secret first (Enter or Cmd+R).");
            return;
        }

        let outcome = match action {
            TransformAction::ShellQuote => Ok((
                shell_quote_escape(&item.content),
                "Shell-quoted to clipboard.",
            )),
            TransformAction::JsonEncode => json_encode_transform(&item.content),
            TransformAction::JsonDecode => json_decode_transform(&item.content),
            TransformAction::UrlEncode => url_encode_transform(&item.content),
            TransformAction::UrlDecode => url_decode_transform(&item.content),
            TransformAction::Base64Encode => base64_encode_transform(&item.content),
            TransformAction::Base64Decode => base64_decode_transform(&item.content),
            TransformAction::PublicCertPemInfo => public_cert_pem_info_transform(&item.content),
        };

        let (transformed, status_message) = match outcome {
            Ok(result) => result,
            Err(err) => {
                show_macos_notification("Pasta", &format!("Transform failed: {err}"));
                return;
            }
        };

        self.mark_self_clipboard_write(&transformed, cx);
        cx.write_to_clipboard(ClipboardItem::new_string(transformed.clone()));

        let mut notification = status_message.to_owned();
        if let Err(err) = self.storage.upsert_clipboard_item(&transformed) {
            eprintln!("warning: failed to store transformed clipboard item: {err}");
            notification.push_str(" Stored in clipboard only.");
        }

        self.query.clear();
        self.query_select_all = false;
        self.query_refresh_due_at = None;
        self.transform_menu_open = false;
        self.selected_index = 0;
        self.selection_changed_at = Instant::now();
        self.refresh_items();
        if !self.items.is_empty() {
            self.results_scroll.scroll_to_item(0);
        }

        show_macos_notification("Pasta", &notification);
        cx.notify();
    }

    pub(crate) fn handle_keystroke(&mut self, event: &KeystrokeEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        let no_modifiers = !modifiers.modified();
        let platform_only =
            modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.function;
        let command_navigation = modifiers.platform
            && !modifiers.shift
            && !modifiers.control
            && !modifiers.alt
            && !modifiers.function;
        let typed_char = typed_character(event);

        if self.info_editor_target_id.is_some() {
            self.handle_info_editor_keystroke(event, cx);
            return;
        }

        if self.parameter_fill_target_id.is_some() {
            self.handle_parameter_fill_keystroke(event, cx);
            return;
        }

        if self.parameter_editor_target_id.is_some() {
            self.handle_parameter_editor_keystroke(event, cx);
            return;
        }

        if self.tag_editor_target_id.is_some() {
            self.handle_tag_editor_keystroke(event, cx);
            return;
        }

        if self.transform_menu_open {
            self.handle_transform_keystroke(event, cx);
            return;
        }

        let is_query_edit_key = (key == "backspace" && no_modifiers) || typed_char.is_some();
        if !is_query_edit_key && self.query_refresh_due_at.is_some() {
            self.flush_pending_query_refresh();
        }

        if command_navigation {
            match key {
                "j" | ";" | "semicolon" => {
                    self.move_selection(1, cx);
                    return;
                }
                "k" | "l" => {
                    self.move_selection(-1, cx);
                    return;
                }
                _ => {}
            }
        }

        if key == "escape" || key == "esc" {
            self.begin_close_transition(LauncherExitIntent::Hide);
            cx.notify();
            return;
        }

        match key {
            "up" | "arrowup" => {
                self.move_selection(-1, cx);
                return;
            }
            "down" | "arrowdown" => {
                self.move_selection(1, cx);
                return;
            }
            "tab" if no_modifiers => {
                self.toggle_transform_menu(cx);
                return;
            }
            "enter" | "return" => {
                self.copy_selected_to_clipboard(cx);
                return;
            }
            "delete" | "forwarddelete" => {
                self.delete_selected_item(cx);
                return;
            }
            "d" if modifiers.platform
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.function =>
            {
                self.delete_selected_item(cx);
                return;
            }
            "r" if modifiers.platform
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.function =>
            {
                self.reveal_and_copy_selected_secret(cx);
                return;
            }
            "s" if modifiers.platform
                && modifiers.shift
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.function =>
            {
                self.mark_selected_item_as_secret(cx);
                return;
            }
            "h" if modifiers.platform
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.function =>
            {
                self.show_command_help = !self.show_command_help;
                cx.notify();
                return;
            }
            "t" if modifiers.platform
                && modifiers.shift
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.function =>
            {
                self.remove_custom_tags_from_selected(cx);
                return;
            }
            "t" if modifiers.platform
                && !modifiers.shift
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.function =>
            {
                self.add_custom_tags_to_selected(cx);
                return;
            }
            "p" if modifiers.platform
                && !modifiers.shift
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.function =>
            {
                self.start_parameter_editor_for_selected(cx);
                return;
            }
            "i" if modifiers.platform
                && !modifiers.shift
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.function =>
            {
                self.start_info_editor_for_selected(cx);
                return;
            }
            "q" if modifiers.platform
                && !modifiers.control
                && !modifiers.alt
                && !modifiers.function =>
            {
                self.begin_close_transition(LauncherExitIntent::Hide);
                cx.notify();
                return;
            }
            "backspace"
                if modifiers.platform
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.function =>
            {
                self.delete_selected_item(cx);
                return;
            }
            "a" if platform_only => {
                if !self.query.is_empty() {
                    self.query_select_all = true;
                    cx.notify();
                }
                return;
            }
            "backspace" if no_modifiers => {
                if self.query_select_all {
                    self.update_query(String::new(), cx);
                } else {
                    let mut query = self.query.clone();
                    query.pop();
                    self.update_query(query, cx);
                }
                return;
            }
            _ => {}
        }

        if let Some(character) = typed_char {
            if self.query_select_all {
                self.update_query(character.to_string(), cx);
            } else {
                let mut query = self.query.clone();
                query.push(character);
                self.update_query(query, cx);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn typed_character(event: &KeystrokeEvent) -> Option<char> {
    let modifiers = &event.keystroke.modifiers;
    if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
        return None;
    }

    let key = event.keystroke.key.as_str();
    if key == "space" {
        return Some(' ');
    }

    if let Some(candidate) = event.keystroke.key_char.as_deref() {
        let mut chars = candidate.chars();
        let first = chars.next()?;
        if chars.next().is_none() && !first.is_control() {
            return Some(first);
        }
    }

    if let Some(mapped) = key_name_to_char(key) {
        return Some(mapped);
    }

    let candidate = key;
    let mut chars = candidate.chars();
    let first = chars.next()?;
    if chars.next().is_some() || first.is_control() {
        return None;
    }

    Some(first)
}

#[cfg(target_os = "macos")]
fn key_name_to_char(key: &str) -> Option<char> {
    match key {
        "minus" | "hyphen" => Some('-'),
        "equal" | "equals" => Some('='),
        "comma" => Some(','),
        "period" | "dot" => Some('.'),
        "slash" => Some('/'),
        "backslash" => Some('\\'),
        "semicolon" => Some(';'),
        "quote" | "apostrophe" => Some('\''),
        "grave" | "backtick" => Some('`'),
        "leftbracket" | "openbracket" | "bracketleft" => Some('['),
        "rightbracket" | "closebracket" | "bracketright" => Some(']'),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn is_parameter_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':')
}

#[cfg(target_os = "macos")]
pub(super) fn parameter_clickable_ranges(content: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut current_start: Option<usize> = None;
    for (ix, ch) in content.char_indices() {
        if is_parameter_word_char(ch) {
            if current_start.is_none() {
                current_start = Some(ix);
            }
        } else if let Some(start) = current_start.take()
            && start < ix
        {
            ranges.push(start..ix);
        }
    }
    if let Some(start) = current_start
        && start < content.len()
    {
        ranges.push(start..content.len());
    }
    ranges
}

#[cfg(target_os = "macos")]
fn is_valid_parameter_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
