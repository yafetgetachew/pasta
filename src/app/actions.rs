#[cfg(target_os = "macos")]
use super::state::{
    CachedRowPresentation, SearchRequest, SearchResponse, TextInputState,
};
#[cfg(target_os = "macos")]
use crate::*;
#[cfg(target_os = "macos")]
use serde_json::Value;
#[cfg(target_os = "macos")]
use std::collections::{HashMap, HashSet};
#[cfg(target_os = "macos")]
use toml::Value as TomlValue;

#[cfg(target_os = "macos")]
const TAG_SEARCH_AUTOCOMPLETE_LIMIT: usize = 6;

impl LauncherView {
    pub(crate) fn new(
        storage: Arc<ClipboardStorage>,
        font_family: SharedString,
        surface_alpha: f32,
        syntax_highlighting: bool,
        search_request_tx: mpsc::Sender<SearchRequest>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            storage,
            font_family,
            surface_alpha,
            syntax_highlighting,
            query_input_state: TextInputState::new(cx),
            info_editor_input_state: TextInputState::new(cx),
            tag_editor_input_state: TextInputState::new(cx),
            parameter_name_input_state: TextInputState::new(cx),
            parameter_fill_input_state: TextInputState::new(cx),
            pending_text_input_focus: None,
            results_scroll: UniformListScrollHandle::new(),
            search_request_tx,
            next_search_request_id: 0,
            latest_search_request_id: 0,
            query: String::new(),
            tag_search_suggestions: Vec::new(),
            items: Vec::new(),
            row_presentations: Vec::new(),
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
            info_editor_select_all: false,
            tag_editor_target_id: None,
            tag_editor_input: String::new(),
            tag_editor_select_all: false,
            tag_editor_mode: TagEditorMode::Add,
            parameter_editor_target_id: None,
            parameter_editor_stage: ParameterEditorStage::SelectValue,
            parameter_editor_force_full: true,
            parameter_editor_selected_targets: Vec::new(),
            parameter_editor_name_inputs: Vec::new(),
            parameter_editor_name_focus_index: 0,
            parameter_editor_name_select_all: false,
            parameter_fill_target_id: None,
            parameter_fill_values: Vec::new(),
            parameter_fill_focus_index: 0,
            parameter_fill_select_all: false,
            transform_menu_open: false,
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
        self.tag_search_suggestions.clear();
        self.query_input_state.reset();
        self.info_editor_input_state.reset();
        self.tag_editor_input_state.reset();
        self.parameter_name_input_state.reset();
        self.parameter_fill_input_state.reset();
        self.pending_text_input_focus = Some(TextInputTarget::Query);
        self.selected_index = 0;
        self.selection_changed_at = Instant::now();
        self.items.clear();
        self.row_presentations.clear();
        self.revealed_secret_id = None;
        self.reveal_until = None;
        self.last_reveal_second_bucket = None;
        self.info_editor_target_id = None;
        self.info_editor_input.clear();
        self.info_editor_select_all = false;
        self.tag_editor_target_id = None;
        self.tag_editor_input.clear();
        self.tag_editor_select_all = false;
        self.tag_editor_mode = TagEditorMode::Add;
        self.parameter_editor_target_id = None;
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        self.parameter_editor_force_full = true;
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_select_all = false;
        self.parameter_fill_target_id = None;
        self.parameter_fill_values.clear();
        self.parameter_fill_focus_index = 0;
        self.parameter_fill_select_all = false;
        self.transform_menu_open = false;
        self.blur_close_armed = false;
        self.suppress_auto_hide = false;
        self.suppress_auto_hide_until = None;
        self.show_command_help = false;
        self.last_window_appearance = None;
        self.request_search();
    }

    pub(crate) fn rebuild_row_presentations(&mut self) {
        self.row_presentations = self
            .items
            .iter()
            .map(CachedRowPresentation::from_record)
            .collect();
    }

    pub(crate) fn set_items(&mut self, items: Vec<ClipboardRecord>) {
        self.items = items;
        self.rebuild_row_presentations();
        if self.selected_index >= self.items.len() {
            self.selected_index = 0;
        }
    }

    pub(crate) fn reset_results_scroll_to_top(&mut self) {
        self.results_scroll
            .scroll_to_item_strict(0, ScrollStrategy::Top);
    }

    pub(crate) fn queue_text_input_focus(&mut self, target: TextInputTarget) {
        self.pending_text_input_focus = Some(target);
    }

    pub(crate) fn query_did_change(&mut self, cx: &mut Context<Self>) {
        self.selected_index = 0;
        self.mark_selection_changed(cx);
        self.reset_results_scroll_to_top();
        self.refresh_tag_search_suggestions();
        self.schedule_query_refresh();
        cx.notify();
    }

    fn refresh_tag_search_suggestions(&mut self) {
        self.tag_search_suggestions = self
            .storage
            .suggest_search_tags(&self.query, TAG_SEARCH_AUTOCOMPLETE_LIMIT);
    }

    fn set_query_text(&mut self, query: String) {
        self.set_text_input_content(TextInputTarget::Query, query);
        let cursor = self.query.len();
        self.query_input_state.selected_range = cursor..cursor;
        self.query_input_state.selection_reversed = false;
        self.query_input_state.marked_range = None;
    }

    pub(crate) fn apply_tag_search_suggestion_index(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(suggestion) = self.tag_search_suggestions.get(index).cloned() else {
            return;
        };
        let Some(next_query) = apply_tag_search_suggestion_to_query(&self.query, &suggestion) else {
            return;
        };
        if next_query == self.query {
            return;
        }

        self.set_query_text(next_query);
        self.queue_text_input_focus(TextInputTarget::Query);
        self.query_did_change(cx);
    }

    fn accept_tag_search_autocomplete(&mut self, cx: &mut Context<Self>) -> bool {
        if self.tag_search_suggestions.is_empty() {
            return false;
        }

        self.apply_tag_search_suggestion_index(0, cx);
        true
    }

    pub(crate) fn mark_selection_changed(&mut self, cx: &mut Context<Self>) {
        self.selection_changed_at = Instant::now();
        self.schedule_preview_settle(cx);
    }

    fn schedule_preview_settle(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(PREVIEW_SETTLE_DELAY_MS))
                .await;
            let _ = this.update(cx, |view, cx| {
                if Instant::now().duration_since(view.selection_changed_at)
                    >= Duration::from_millis(PREVIEW_SETTLE_DELAY_MS)
                {
                    cx.notify();
                }
            });
        })
        .detach();
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
        let items = self
            .storage
            .search_items(&self.query, 48)
            .unwrap_or_else(|_| Vec::new());
        self.set_items(items);
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

    pub(crate) fn apply_search_response(&mut self, response: SearchResponse) -> bool {
        if response.request_id < self.latest_search_request_id {
            return false;
        }

        self.set_items(response.items);
        if self.selected_index == 0 {
            self.reset_results_scroll_to_top();
        }
        true
    }

    pub(crate) fn schedule_query_refresh(&mut self) {
        self.request_search();
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
            self.mark_selection_changed(cx);
            self.results_scroll
                .scroll_to_item(self.selected_index, ScrollStrategy::Center);
            cx.notify();
        }
    }

    pub(crate) fn select_result_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            self.selected_index = 0;
            return;
        }

        let next_index = index.min(self.items.len().saturating_sub(1));
        let changed = self.selected_index != next_index;
        self.selected_index = next_index;
        self.results_scroll
            .scroll_to_item(self.selected_index, ScrollStrategy::Center);
        if changed {
            self.mark_selection_changed(cx);
        }
        cx.notify();
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
        self.results_scroll
            .scroll_to_item(self.selected_index, ScrollStrategy::Center);
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
                    self.results_scroll
                        .scroll_to_item(self.selected_index, ScrollStrategy::Center);
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
                    self.results_scroll
                        .scroll_to_item(self.selected_index, ScrollStrategy::Center);
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

    pub(crate) fn start_info_editor_for_selected(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(self.selected_index).cloned() else {
            return;
        };

        self.info_editor_target_id = Some(item.id);
        self.set_text_input_content(TextInputTarget::InfoEditor, item.description);
        self.info_editor_input_state.reset();
        let cursor = self.info_editor_input.len();
        self.info_editor_input_state.selected_range = cursor..cursor;
        self.info_editor_select_all = false;
        self.tag_editor_target_id = None;
        self.tag_editor_input.clear();
        self.tag_editor_input_state.reset();
        self.tag_editor_select_all = false;
        self.tag_editor_mode = TagEditorMode::Add;
        self.parameter_editor_target_id = None;
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_name_input_state.reset();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_select_all = false;
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        self.parameter_editor_force_full = true;
        self.parameter_fill_target_id = None;
        self.parameter_fill_values.clear();
        self.parameter_fill_input_state.reset();
        self.parameter_fill_focus_index = 0;
        self.parameter_fill_select_all = false;
        self.transform_menu_open = false;
        self.queue_text_input_focus(TextInputTarget::InfoEditor);
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
                    self.results_scroll
                        .scroll_to_item(self.selected_index, ScrollStrategy::Center);
                }
                self.selection_changed_at = Instant::now();
                self.info_editor_target_id = None;
                self.info_editor_input.clear();
                self.info_editor_input_state.reset();
                self.info_editor_select_all = false;
                self.queue_text_input_focus(TextInputTarget::Query);
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
                self.info_editor_input_state.reset();
                self.info_editor_select_all = false;
                self.queue_text_input_focus(TextInputTarget::Query);
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
        self.info_editor_input_state.reset();
        self.info_editor_select_all = false;
        self.queue_text_input_focus(TextInputTarget::Query);
        cx.notify();
    }

    pub(crate) fn add_custom_tags_to_selected(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.items.get(self.selected_index).map(|item| item.id) else {
            return;
        };
        self.info_editor_target_id = None;
        self.info_editor_input.clear();
        self.info_editor_input_state.reset();
        self.info_editor_select_all = false;
        self.tag_editor_mode = TagEditorMode::Add;
        self.tag_editor_target_id = Some(item_id);
        self.tag_editor_input.clear();
        self.tag_editor_input_state.reset();
        self.tag_editor_select_all = false;
        self.queue_text_input_focus(TextInputTarget::TagEditor);
        cx.notify();
    }

    pub(crate) fn remove_custom_tags_from_selected(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.items.get(self.selected_index).map(|item| item.id) else {
            return;
        };
        self.info_editor_target_id = None;
        self.info_editor_input.clear();
        self.info_editor_input_state.reset();
        self.info_editor_select_all = false;
        self.tag_editor_mode = TagEditorMode::Remove;
        self.tag_editor_target_id = Some(item_id);
        self.tag_editor_input.clear();
        self.tag_editor_input_state.reset();
        self.tag_editor_select_all = false;
        self.queue_text_input_focus(TextInputTarget::TagEditor);
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
                    self.results_scroll.scroll_to_item(ix, ScrollStrategy::Center);
                } else if !self.items.is_empty() {
                    self.selected_index = previous_index.min(self.items.len().saturating_sub(1));
                    self.selection_changed_at = Instant::now();
                    self.results_scroll
                        .scroll_to_item(self.selected_index, ScrollStrategy::Center);
                }

                self.tag_editor_target_id = None;
                self.tag_editor_input.clear();
                self.tag_editor_input_state.reset();
                self.tag_editor_select_all = false;
                self.queue_text_input_focus(TextInputTarget::Query);
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
                self.tag_editor_input_state.reset();
                self.tag_editor_select_all = false;
                self.queue_text_input_focus(TextInputTarget::Query);
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
        self.tag_editor_input_state.reset();
        self.tag_editor_select_all = false;
        self.tag_editor_mode = TagEditorMode::Add;
        self.queue_text_input_focus(TextInputTarget::Query);
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
        self.info_editor_input_state.reset();
        self.info_editor_select_all = false;
        self.parameter_editor_target_id = Some(item.id);
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_name_input_state.reset();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_select_all = false;
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        self.parameter_editor_force_full = true;
        self.parameter_fill_target_id = None;
        self.parameter_fill_values.clear();
        self.parameter_fill_input_state.reset();
        self.parameter_fill_focus_index = 0;
        self.parameter_fill_select_all = false;
        self.transform_menu_open = false;
        self.tag_editor_target_id = None;
        self.tag_editor_input_state.reset();
        self.pending_text_input_focus = None;
        cx.notify();
    }

    pub(crate) fn cancel_parameter_editor(&mut self, cx: &mut Context<Self>) {
        self.parameter_editor_target_id = None;
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_name_input_state.reset();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_select_all = false;
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        self.parameter_editor_force_full = true;
        self.queue_text_input_focus(TextInputTarget::Query);
        cx.notify();
    }

    fn reset_parameter_editor_selection_state(&mut self) {
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_name_input_state.reset();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_select_all = false;
    }

    pub(crate) fn set_parameter_editor_full_mode(
        &mut self,
        force_full: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(item_id) = self.parameter_editor_target_id else {
            return;
        };

        if !force_full {
            let Some(content) = self
                .items
                .iter()
                .find(|entry| entry.id == item_id)
                .map(|entry| entry.content.as_str())
            else {
                return;
            };
            if !has_structured_parameter_candidates(content) {
                show_macos_notification(
                    "Pasta",
                    "Guided mode is only available for structured snippets.",
                );
                return;
            }
        }

        if self.parameter_editor_force_full == force_full
            && self.parameter_editor_stage == ParameterEditorStage::SelectValue
        {
            return;
        }

        self.parameter_editor_force_full = force_full;
        self.reset_parameter_editor_selection_state();
        cx.notify();
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
    }

    fn set_parameter_target(&mut self, target: &str, suggested_name: Option<&str>) {
        let normalized = target.trim();
        if normalized.is_empty() {
            return;
        }
        self.parameter_editor_selected_targets = vec![normalized.to_owned()];
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_select_all = false;
        self.sync_parameter_editor_name_inputs();
        self.assign_parameter_name_for_target(normalized, suggested_name);
    }

    fn toggle_parameter_target(&mut self, target: &str, suggested_name: Option<&str>) {
        let normalized = target.trim();
        if normalized.is_empty() {
            return;
        }
        let mut added = false;
        if let Some(ix) = self
            .parameter_editor_selected_targets
            .iter()
            .position(|existing| existing == normalized)
        {
            self.parameter_editor_selected_targets.remove(ix);
        } else {
            self.parameter_editor_selected_targets
                .push(normalized.to_owned());
            added = true;
        }
        self.parameter_editor_name_focus_index = self
            .parameter_editor_selected_targets
            .len()
            .saturating_sub(1);
        self.parameter_editor_name_select_all = false;
        self.sync_parameter_editor_name_inputs();
        if added {
            self.assign_parameter_name_for_target(normalized, suggested_name);
        }
    }

    fn ensure_parameter_editor_target_added(&self) -> bool {
        !self.parameter_editor_selected_targets.is_empty()
    }

    fn assign_parameter_name_for_target(&mut self, target: &str, suggested_name: Option<&str>) {
        let Some(suggested_name) = suggested_name else {
            return;
        };
        let normalized_name = suggested_name.trim();
        if normalized_name.is_empty() {
            return;
        }
        let Some(ix) = self
            .parameter_editor_selected_targets
            .iter()
            .position(|entry| entry == target)
        else {
            return;
        };
        if self.parameter_editor_name_inputs.len() <= ix {
            self.sync_parameter_editor_name_inputs();
        }
        if let Some(active) = self.parameter_editor_name_inputs.get_mut(ix) {
            *active = normalized_name.to_owned();
        }
    }

    fn has_valid_parameter_names_selected(&mut self) -> bool {
        if self.parameter_editor_selected_targets.is_empty() {
            return false;
        }

        if self.parameter_editor_name_inputs.len() != self.parameter_editor_selected_targets.len() {
            self.sync_parameter_editor_name_inputs();
        }
        if self.parameter_editor_name_inputs.len() != self.parameter_editor_selected_targets.len() {
            return false;
        }

        let mut seen = HashSet::new();
        for name in &self.parameter_editor_name_inputs {
            let trimmed = name.trim();
            if !is_valid_parameter_name(trimmed) {
                return false;
            }
            if !seen.insert(trimmed.to_ascii_lowercase()) {
                return false;
            }
        }

        true
    }

    fn continue_parameter_editor_after_selection(&mut self, cx: &mut Context<Self>) {
        if !self.ensure_parameter_editor_target_added() {
            show_macos_notification("Pasta", "Select one or more parameter targets first.");
            return;
        }

        if self.has_valid_parameter_names_selected() {
            self.commit_parameter_editor(cx);
            return;
        }

        self.parameter_editor_stage = ParameterEditorStage::EnterName;
        self.sync_parameter_editor_name_inputs();
        self.parameter_editor_name_focus_index =
            first_parameter_name_issue_index(&self.parameter_editor_name_inputs);
        if let Some(active_name) = self
            .parameter_editor_name_inputs
            .get(self.parameter_editor_name_focus_index)
        {
            let cursor = active_name.len();
            self.parameter_name_input_state.selected_range = cursor..cursor;
        }
        self.parameter_name_input_state.selection_reversed = false;
        self.parameter_name_input_state.marked_range = None;
        self.parameter_editor_name_select_all = false;
        self.queue_text_input_focus(TextInputTarget::ParameterName);
        cx.notify();
    }

    fn focus_next_parameter_name_input(&mut self) {
        if self.parameter_editor_name_inputs.is_empty() {
            self.parameter_editor_name_focus_index = 0;
        } else {
            self.parameter_editor_name_focus_index = (self.parameter_editor_name_focus_index + 1)
                % self.parameter_editor_name_inputs.len();
        }
        self.parameter_editor_name_select_all = false;
        if let Some(active_name) = self
            .parameter_editor_name_inputs
            .get(self.parameter_editor_name_focus_index)
        {
            let cursor = active_name.len();
            self.parameter_name_input_state.selected_range = cursor..cursor;
        }
        self.parameter_name_input_state.selection_reversed = false;
        self.parameter_name_input_state.marked_range = None;
        self.queue_text_input_focus(TextInputTarget::ParameterName);
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
        self.parameter_editor_name_select_all = false;
        if let Some(active_name) = self
            .parameter_editor_name_inputs
            .get(self.parameter_editor_name_focus_index)
        {
            let cursor = active_name.len();
            self.parameter_name_input_state.selected_range = cursor..cursor;
        }
        self.parameter_name_input_state.selection_reversed = false;
        self.parameter_name_input_state.marked_range = None;
        self.queue_text_input_focus(TextInputTarget::ParameterName);
    }

    pub(crate) fn focus_parameter_name_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.parameter_editor_name_inputs.is_empty() {
            return;
        }
        self.parameter_editor_name_focus_index =
            index.min(self.parameter_editor_name_inputs.len() - 1);
        if let Some(active_name) = self
            .parameter_editor_name_inputs
            .get(self.parameter_editor_name_focus_index)
        {
            let cursor = active_name.len();
            self.parameter_name_input_state.selected_range = cursor..cursor;
        }
        self.parameter_name_input_state.selection_reversed = false;
        self.parameter_name_input_state.marked_range = None;
        self.parameter_editor_name_select_all = false;
        self.queue_text_input_focus(TextInputTarget::ParameterName);
        cx.notify();
    }

    pub(crate) fn commit_parameter_editor(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.parameter_editor_target_id else {
            return;
        };
        if self.parameter_editor_selected_targets.is_empty() {
            show_macos_notification("Pasta", "Select one or more parameter targets first.");
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
                show_macos_notification(
                    "Pasta",
                    "Each parameter name must start with a letter/underscore and use letters, numbers, or underscores.",
                );
                cx.notify();
                return;
            }
            if !seen_names.insert(trimmed.to_ascii_lowercase()) {
                self.parameter_editor_name_focus_index = ix;
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
                self.results_scroll
                    .scroll_to_item(self.selected_index, ScrollStrategy::Center);
            }
            self.selection_changed_at = Instant::now();
            self.parameter_editor_target_id = None;
            self.parameter_editor_selected_targets.clear();
            self.parameter_editor_name_inputs.clear();
            self.parameter_name_input_state.reset();
            self.parameter_editor_name_focus_index = 0;
            self.parameter_editor_name_select_all = false;
            self.parameter_editor_stage = ParameterEditorStage::SelectValue;
            self.parameter_editor_force_full = true;
            self.queue_text_input_focus(TextInputTarget::Query);
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
        let candidates = parameter_clickable_candidates(&content, self.parameter_editor_force_full);
        let Some(candidate) = candidates.get(range_index) else {
            return;
        };
        let target = candidate.target.as_str();
        let suggested_name = candidate.suggested_name.as_deref();

        if additive {
            self.toggle_parameter_target(target, suggested_name);
        } else {
            self.set_parameter_target(target, suggested_name);
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
            return;
        }

        match key {
            "escape" | "esc" => {
                self.cancel_parameter_editor(cx);
            }
            "g" if no_modifiers => {
                self.set_parameter_editor_full_mode(false, cx);
            }
            "f" if no_modifiers => {
                self.set_parameter_editor_full_mode(true, cx);
            }
            "tab" if no_modifiers || shift_only => {
                self.continue_parameter_editor_after_selection(cx);
            }
            "enter" | "return" => {
                self.continue_parameter_editor_after_selection(cx);
            }
            "p" if no_modifiers => {
                self.continue_parameter_editor_after_selection(cx);
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
        self.info_editor_input_state.reset();
        self.info_editor_select_all = false;
        self.parameter_fill_target_id = Some(item_id);
        self.parameter_fill_values = vec![String::new(); parameters.len()];
        self.parameter_fill_input_state.reset();
        self.parameter_fill_focus_index = 0;
        self.parameter_fill_select_all = false;
        self.parameter_editor_target_id = None;
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_name_input_state.reset();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_select_all = false;
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        self.parameter_editor_force_full = true;
        self.transform_menu_open = false;
        self.tag_editor_target_id = None;
        self.tag_editor_input_state.reset();
        if !self.parameter_fill_values.is_empty() {
            self.queue_text_input_focus(TextInputTarget::ParameterFill);
        } else {
            self.queue_text_input_focus(TextInputTarget::Query);
        }
        cx.notify();
    }

    pub(crate) fn cancel_parameter_fill_prompt(&mut self, cx: &mut Context<Self>) {
        self.parameter_fill_target_id = None;
        self.parameter_fill_values.clear();
        self.parameter_fill_input_state.reset();
        self.parameter_fill_focus_index = 0;
        self.parameter_fill_select_all = false;
        self.queue_text_input_focus(TextInputTarget::Query);
        cx.notify();
    }

    pub(crate) fn focus_parameter_fill_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.parameter_fill_values.is_empty() {
            return;
        }
        self.parameter_fill_focus_index = index.min(self.parameter_fill_values.len() - 1);
        if let Some(active_value) = self.parameter_fill_values.get(self.parameter_fill_focus_index) {
            let cursor = active_value.len();
            self.parameter_fill_input_state.selected_range = cursor..cursor;
        }
        self.parameter_fill_input_state.selection_reversed = false;
        self.parameter_fill_input_state.marked_range = None;
        self.parameter_fill_select_all = false;
        self.queue_text_input_focus(TextInputTarget::ParameterFill);
        cx.notify();
    }

    pub(crate) fn commit_parameter_fill_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.parameter_fill_target_id else {
            return;
        };
        let Some(item) = self.items.iter().find(|entry| entry.id == item_id).cloned() else {
            self.parameter_fill_target_id = None;
            self.parameter_fill_values.clear();
            self.parameter_fill_input_state.reset();
            self.parameter_fill_focus_index = 0;
            self.parameter_fill_select_all = false;
            self.queue_text_input_focus(TextInputTarget::Query);
            cx.notify();
            return;
        };

        if self.parameter_fill_values.len() != item.parameters.len() {
            self.parameter_fill_values = vec![String::new(); item.parameters.len()];
            self.parameter_fill_input_state.reset();
            self.parameter_fill_focus_index = 0;
            self.parameter_fill_select_all = false;
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
        self.parameter_fill_values.clear();
        self.parameter_fill_input_state.reset();
        self.parameter_fill_focus_index = 0;
        self.parameter_fill_select_all = false;
        self.queue_text_input_focus(TextInputTarget::Query);
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
        let shift_only = modifiers.shift && !modifiers.platform && !modifiers.control;
        if self.parameter_fill_values.is_empty() && key != "escape" && key != "esc" {
            return;
        }

        match key {
            "escape" | "esc" => {
                self.cancel_parameter_fill_prompt(cx);
                return;
            }
            "tab" if no_modifiers => {
                self.parameter_fill_focus_index =
                    (self.parameter_fill_focus_index + 1) % self.parameter_fill_values.len();
                self.parameter_fill_select_all = false;
                cx.notify();
                return;
            }
            "tab" if shift_only => {
                if self.parameter_fill_focus_index == 0 {
                    self.parameter_fill_focus_index = self.parameter_fill_values.len() - 1;
                } else {
                    self.parameter_fill_focus_index -= 1;
                }
                self.parameter_fill_select_all = false;
                cx.notify();
                return;
            }
            "up" | "arrowup" => {
                if self.parameter_fill_focus_index == 0 {
                    self.parameter_fill_focus_index = self.parameter_fill_values.len() - 1;
                } else {
                    self.parameter_fill_focus_index -= 1;
                }
                self.parameter_fill_select_all = false;
                cx.notify();
                return;
            }
            "down" | "arrowdown" => {
                self.parameter_fill_focus_index =
                    (self.parameter_fill_focus_index + 1) % self.parameter_fill_values.len();
                self.parameter_fill_select_all = false;
                cx.notify();
                return;
            }
            "enter" | "return" => {
                self.commit_parameter_fill_prompt(cx);
                return;
            }
            _ => {}
        }
    }

    pub(crate) fn handle_info_editor_keystroke(
        &mut self,
        event: &KeystrokeEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();

        match key {
            "escape" | "esc" => {
                self.cancel_info_editor(cx);
                return;
            }
            "enter" | "return" => {
                self.commit_info_editor(cx);
                return;
            }
            _ => {}
        }
    }

    pub(crate) fn handle_tag_editor_keystroke(
        &mut self,
        event: &KeystrokeEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();

        match key {
            "escape" | "esc" => {
                self.cancel_custom_tags_editor(cx);
                return;
            }
            "enter" | "return" => {
                self.commit_custom_tags(cx);
                return;
            }
            _ => {}
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
        self.tag_search_suggestions.clear();
        let query_cursor = self.query.len();
        self.query_input_state.selected_range = query_cursor..query_cursor;
        self.query_input_state.selection_reversed = false;
        self.query_input_state.marked_range = None;
        self.transform_menu_open = false;
        self.selected_index = 0;
        self.selection_changed_at = Instant::now();
        self.refresh_items();
        if !self.items.is_empty() {
            self.results_scroll.scroll_to_item(0, ScrollStrategy::Top);
        }

        show_macos_notification("Pasta", &notification);
        cx.notify();
    }

    pub(crate) fn handle_keystroke(&mut self, event: &KeystrokeEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        let no_modifiers = !modifiers.modified();
        let command_navigation = modifiers.platform
            && !modifiers.shift
            && !modifiers.control
            && !modifiers.alt
            && !modifiers.function;

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

        if key == "tab" && no_modifiers && self.accept_tag_search_autocomplete(cx) {
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
            _ => {}
        }
    }
}

#[cfg(target_os = "macos")]
fn is_parameter_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':')
}

#[cfg(target_os = "macos")]
fn first_parameter_name_issue_index(names: &[String]) -> usize {
    let mut seen = HashSet::new();

    for (ix, name) in names.iter().enumerate() {
        let trimmed = name.trim();
        if !is_valid_parameter_name(trimmed) {
            return ix;
        }

        let normalized = trimmed.to_ascii_lowercase();
        if !seen.insert(normalized) {
            return ix;
        }
    }

    0
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub(super) struct ParameterClickableCandidate {
    pub(super) target: String,
    pub(super) label: String,
    pub(super) suggested_name: Option<String>,
}

#[cfg(target_os = "macos")]
pub(super) fn parameter_clickable_candidates(
    content: &str,
    force_full: bool,
) -> Vec<ParameterClickableCandidate> {
    if force_full {
        return dedupe_parameter_candidates(parameter_word_candidates(content));
    }

    let structured_candidates = parameter_structured_candidates(content);
    if !structured_candidates.is_empty() {
        return dedupe_parameter_candidates(structured_candidates);
    }

    let assignment_candidates = parameter_assignment_line_candidates(content);
    if !assignment_candidates.is_empty() {
        return dedupe_parameter_candidates(assignment_candidates);
    }

    dedupe_parameter_candidates(parameter_word_candidates(content))
}

#[cfg(target_os = "macos")]
pub(super) fn has_structured_parameter_candidates(content: &str) -> bool {
    !parameter_structured_candidates(content).is_empty()
}

#[cfg(target_os = "macos")]
fn parameter_structured_candidates(content: &str) -> Vec<ParameterClickableCandidate> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if let Ok(json_value) = serde_json::from_str::<Value>(trimmed) {
        let mut candidates = Vec::new();
        collect_json_parameter_candidates(&json_value, String::new(), &mut candidates, 0);
        if !candidates.is_empty() {
            return candidates;
        }
    }

    let toml_candidates = parameter_toml_scalar_candidates(trimmed);
    if !toml_candidates.is_empty() {
        return toml_candidates;
    }

    let yaml_candidates = parameter_yaml_scalar_candidates(content);
    if !yaml_candidates.is_empty() {
        return yaml_candidates;
    }

    let xml_candidates = parameter_xml_scalar_candidates(content);
    if !xml_candidates.is_empty() {
        return xml_candidates;
    }

    Vec::new()
}

#[cfg(target_os = "macos")]
fn collect_json_parameter_candidates(
    value: &Value,
    path: String,
    candidates: &mut Vec<ParameterClickableCandidate>,
    depth: usize,
) {
    if depth > 14 || candidates.len() >= 220 {
        return;
    }

    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let next_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                collect_json_parameter_candidates(child, next_path, candidates, depth + 1);
            }
        }
        Value::Array(items) => {
            for (ix, child) in items.iter().take(24).enumerate() {
                let next_path = if path.is_empty() {
                    format!("[{ix}]")
                } else {
                    format!("{path}[{ix}]")
                };
                collect_json_parameter_candidates(child, next_path, candidates, depth + 1);
            }
        }
        Value::String(text) => {
            push_structured_parameter_candidate(candidates, path, text.to_owned());
        }
        Value::Number(number) => {
            push_structured_parameter_candidate(candidates, path, number.to_string());
        }
        Value::Bool(flag) => {
            push_structured_parameter_candidate(
                candidates,
                path,
                if *flag { "true" } else { "false" }.to_owned(),
            );
        }
        Value::Null => {}
    }
}

#[cfg(target_os = "macos")]
fn parameter_toml_scalar_candidates(content: &str) -> Vec<ParameterClickableCandidate> {
    let Ok(toml_value) = toml::from_str::<TomlValue>(content) else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    collect_toml_parameter_candidates(&toml_value, String::new(), &mut candidates, 0);
    candidates
}

#[cfg(target_os = "macos")]
fn collect_toml_parameter_candidates(
    value: &TomlValue,
    path: String,
    candidates: &mut Vec<ParameterClickableCandidate>,
    depth: usize,
) {
    if depth > 14 || candidates.len() >= 220 {
        return;
    }

    match value {
        TomlValue::Table(table) => {
            for (key, child) in table {
                let next_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                collect_toml_parameter_candidates(child, next_path, candidates, depth + 1);
            }
        }
        TomlValue::Array(items) => {
            for (ix, child) in items.iter().take(24).enumerate() {
                let next_path = if path.is_empty() {
                    format!("[{ix}]")
                } else {
                    format!("{path}[{ix}]")
                };
                collect_toml_parameter_candidates(child, next_path, candidates, depth + 1);
            }
        }
        TomlValue::String(value) => {
            push_structured_parameter_candidate(candidates, path, value.to_owned());
        }
        TomlValue::Integer(value) => {
            push_structured_parameter_candidate(candidates, path, value.to_string());
        }
        TomlValue::Float(value) => {
            push_structured_parameter_candidate(candidates, path, value.to_string());
        }
        TomlValue::Boolean(value) => {
            push_structured_parameter_candidate(
                candidates,
                path,
                if *value { "true" } else { "false" }.to_owned(),
            );
        }
        TomlValue::Datetime(value) => {
            push_structured_parameter_candidate(candidates, path, value.to_string());
        }
    }
}

#[cfg(target_os = "macos")]
fn parameter_yaml_scalar_candidates(content: &str) -> Vec<ParameterClickableCandidate> {
    if !looks_like_yaml_document(content) {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    let mut path_stack: Vec<(usize, String)> = Vec::new();

    for line in content.lines().take(700) {
        let trimmed_end = line.trim_end();
        let trimmed = trimmed_end.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed == "---"
            || trimmed == "..."
            || trimmed.starts_with('{')
            || trimmed.starts_with('[')
        {
            continue;
        }

        let indent = trimmed_end.len().saturating_sub(trimmed.len());
        while path_stack.last().is_some_and(|(level, _)| *level >= indent) {
            path_stack.pop();
        }

        let mut body = trimmed;
        if let Some(rest) = body.strip_prefix("- ") {
            body = rest.trim_start();
        }

        if body.ends_with(':') {
            let key = body.trim_end_matches(':').trim();
            if is_parameter_key_token(key) {
                path_stack.push((indent, key.to_owned()));
            }
            continue;
        }

        let Some((raw_key, raw_value)) = body.split_once(':') else {
            continue;
        };
        let key = raw_key.trim();
        if !is_parameter_key_token(key) {
            continue;
        }

        let value = raw_value.trim();
        if let Some(normalized) = normalize_yaml_scalar(value) {
            let mut path = path_stack
                .iter()
                .map(|(_, segment)| segment.as_str())
                .collect::<Vec<_>>()
                .join(".");
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(key);
            push_structured_parameter_candidate(&mut candidates, path, normalized);
        } else {
            path_stack.push((indent, key.to_owned()));
        }
    }

    candidates
}

#[cfg(target_os = "macos")]
fn normalize_yaml_scalar(raw: &str) -> Option<String> {
    let mut value = raw.trim();
    if value.is_empty() {
        return None;
    }

    if let Some((without_comment, _)) = value.split_once(" #") {
        value = without_comment.trim_end();
    }
    if value.is_empty() {
        return None;
    }

    if matches!(value, "|" | ">" | "{}" | "[]" | "null" | "~") {
        return None;
    }

    let stripped = value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|inner| inner.strip_suffix('\''))
        })
        .unwrap_or(value)
        .trim();
    if stripped.is_empty() {
        return None;
    }
    Some(stripped.to_owned())
}

#[cfg(target_os = "macos")]
fn looks_like_yaml_document(content: &str) -> bool {
    let mut yamlish_lines = 0_usize;
    let mut other_lines = 0_usize;

    for line in content.lines().take(320) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" || trimmed == "..." {
            continue;
        }

        let mut body = trimmed;
        if let Some(rest) = body.strip_prefix("- ") {
            body = rest.trim_start();
        }
        if body.is_empty() {
            continue;
        }

        if body.ends_with(':') {
            let key = body.trim_end_matches(':').trim();
            if is_parameter_key_token(key) {
                yamlish_lines += 1;
                continue;
            }
        }

        if let Some((raw_key, _)) = body.split_once(':')
            && is_parameter_key_token(raw_key.trim())
        {
            yamlish_lines += 1;
            continue;
        }

        other_lines += 1;
        if other_lines > 90 {
            break;
        }
    }

    yamlish_lines >= 2 && yamlish_lines >= other_lines
}

#[cfg(target_os = "macos")]
fn parameter_xml_scalar_candidates(content: &str) -> Vec<ParameterClickableCandidate> {
    let trimmed = content.trim();
    if !trimmed.starts_with('<') || !trimmed.contains("</") {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    for line in content.lines().take(700) {
        let trimmed = line.trim();
        if trimmed.len() < 6 || !trimmed.starts_with('<') || trimmed.starts_with("</") {
            continue;
        }
        if trimmed.starts_with("<?") || trimmed.starts_with("<!--") {
            continue;
        }
        if let Some((tag, value)) = parse_xml_tag_value(trimmed) {
            push_structured_parameter_candidate(&mut candidates, tag, value);
        }
    }
    candidates
}

#[cfg(target_os = "macos")]
fn parse_xml_tag_value(line: &str) -> Option<(String, String)> {
    let open_end = line.find('>')?;
    if open_end <= 1 {
        return None;
    }

    let raw_open = &line[1..open_end];
    if raw_open.starts_with('/') {
        return None;
    }
    let tag = raw_open.split_whitespace().next()?.trim();
    if tag.is_empty() || tag.ends_with('/') {
        return None;
    }

    let close_tag = format!("</{tag}>");
    if !line.ends_with(&close_tag) {
        return None;
    }

    let inner_start = open_end + 1;
    let inner_end = line.len().saturating_sub(close_tag.len());
    if inner_start >= inner_end {
        return None;
    }
    let value = line[inner_start..inner_end].trim();
    if value.is_empty() || value.contains('<') {
        return None;
    }
    Some((tag.to_owned(), value.to_owned()))
}

#[cfg(target_os = "macos")]
fn parameter_assignment_line_candidates(content: &str) -> Vec<ParameterClickableCandidate> {
    let mut candidates = Vec::new();
    for line in content.lines().take(700) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let separator_index = trimmed.find('=').or_else(|| trimmed.find(':'));
        let Some(separator_index) = separator_index else {
            continue;
        };
        if separator_index == 0 {
            continue;
        }

        let key = trimmed[..separator_index]
            .trim()
            .trim_matches('"')
            .trim_matches('\'');
        if !is_parameter_key_token(key) {
            continue;
        }

        let value = trimmed[separator_index + 1..]
            .trim()
            .trim_matches('"')
            .trim_matches('\'');
        if value.is_empty() || value.len() > 160 {
            continue;
        }

        push_freetext_parameter_candidate(&mut candidates, Some(key), value);
    }
    candidates
}

#[cfg(target_os = "macos")]
fn parameter_word_candidates(content: &str) -> Vec<ParameterClickableCandidate> {
    let mut candidates = Vec::new();
    let mut current_start: Option<usize> = None;
    for (ix, ch) in content.char_indices() {
        if is_parameter_word_char(ch) {
            if current_start.is_none() {
                current_start = Some(ix);
            }
        } else if let Some(start) = current_start.take()
            && start < ix
            && let Some(token) = content.get(start..ix)
        {
            push_freetext_parameter_candidate(&mut candidates, None, token);
        }
    }
    if let Some(start) = current_start
        && start < content.len()
        && let Some(token) = content.get(start..content.len())
    {
        push_freetext_parameter_candidate(&mut candidates, None, token);
    }
    candidates
}

#[cfg(target_os = "macos")]
fn dedupe_parameter_candidates(
    candidates: Vec<ParameterClickableCandidate>,
) -> Vec<ParameterClickableCandidate> {
    let mut seen_targets = HashSet::new();
    let mut deduped = Vec::new();
    for candidate in candidates {
        let normalized_target = candidate.target.trim();
        if normalized_target.is_empty() {
            continue;
        }
        if !seen_targets.insert(normalized_target.to_ascii_lowercase()) {
            continue;
        }
        deduped.push(ParameterClickableCandidate {
            target: normalized_target.to_owned(),
            label: candidate.label,
            suggested_name: candidate.suggested_name,
        });
        if deduped.len() >= 180 {
            break;
        }
    }
    deduped
}

#[cfg(target_os = "macos")]
fn push_structured_parameter_candidate(
    candidates: &mut Vec<ParameterClickableCandidate>,
    key_or_path: String,
    target: String,
) {
    let normalized_target = target.trim();
    let key_or_path = key_or_path.trim();
    if normalized_target.is_empty() || normalized_target.len() > 180 || key_or_path.is_empty() {
        return;
    }

    let Some(suggested_name) = parameter_name_from_path(key_or_path) else {
        return;
    };
    let label = preview_for_parameter_candidate(key_or_path, 54);

    candidates.push(ParameterClickableCandidate {
        target: normalized_target.to_owned(),
        label,
        suggested_name: Some(suggested_name),
    });
}

#[cfg(target_os = "macos")]
fn push_freetext_parameter_candidate(
    candidates: &mut Vec<ParameterClickableCandidate>,
    key: Option<&str>,
    target: &str,
) {
    let normalized_target = target.trim();
    if normalized_target.is_empty() || normalized_target.len() > 180 {
        return;
    }

    let value_preview = preview_for_parameter_candidate(normalized_target, 28);
    let label = if let Some(key) = key.map(str::trim).filter(|key| !key.is_empty()) {
        preview_for_parameter_candidate(&format!("{key} = {value_preview}"), 54)
    } else {
        value_preview
    };
    candidates.push(ParameterClickableCandidate {
        target: normalized_target.to_owned(),
        label,
        suggested_name: None,
    });
}

#[cfg(target_os = "macos")]
fn parameter_name_from_path(path: &str) -> Option<String> {
    let mut output = String::new();
    let mut previous_was_separator = false;

    for ch in path.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
            continue;
        }

        if !previous_was_separator {
            output.push('_');
            previous_was_separator = true;
        }
    }

    let trimmed = output.trim_matches('_');
    if trimmed.is_empty() {
        return None;
    }

    let mut normalized = trimmed.to_owned();
    if normalized
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        normalized.insert(0, '_');
    }

    if is_valid_parameter_name(&normalized) {
        Some(normalized)
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn preview_for_parameter_candidate(value: &str, max_chars: usize) -> String {
    let collapsed = value
        .replace(['\n', '\r', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut chars = collapsed.chars();
    let preview: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

#[cfg(target_os = "macos")]
fn apply_tag_search_suggestion_to_query(query: &str, suggestion: &str) -> Option<String> {
    let normalized_suggestion = suggestion.trim().to_ascii_lowercase();
    if normalized_suggestion.is_empty() {
        return None;
    }

    let normalized_query = query.trim_start().to_ascii_lowercase();
    if !normalized_query.starts_with(':') {
        return None;
    }

    let effective_query = normalized_query.trim_start_matches(':');
    let ends_with_whitespace = effective_query
        .chars()
        .last()
        .is_some_and(|ch| ch.is_whitespace());
    let mut terms: Vec<String> = effective_query
        .split_whitespace()
        .map(|term| term.trim_start_matches(':').trim().to_owned())
        .filter(|term| !term.is_empty())
        .collect();

    if !ends_with_whitespace && !terms.is_empty() {
        terms.pop();
    }

    if terms.iter().any(|term| term == &normalized_suggestion) {
        return Some(format!(":{}", terms.join(" ")));
    }

    terms.push(normalized_suggestion);
    Some(format!(":{}", terms.join(" ")))
}

#[cfg(target_os = "macos")]
fn is_parameter_key_token(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 80
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
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

#[cfg(all(target_os = "macos", test))]
mod tests {
    use super::*;

    #[test]
    fn json_candidates_auto_name_from_field_paths() {
        let json = r#"{
            "id": "101",
            "username": "jdoe",
            "email": "jdoe@example.com",
            "location": { "city": "New York" }
        }"#;

        let candidates = parameter_clickable_candidates(json, false);
        assert!(candidates.iter().any(|candidate| {
            candidate.label == "id"
                && candidate.target == "101"
                && candidate.suggested_name.as_deref() == Some("id")
        }));
        assert!(candidates.iter().any(|candidate| {
            candidate.label == "username"
                && candidate.target == "jdoe"
                && candidate.suggested_name.as_deref() == Some("username")
        }));
        assert!(candidates.iter().any(|candidate| {
            candidate.label == "location.city"
                && candidate.target == "New York"
                && candidate.suggested_name.as_deref() == Some("location_city")
        }));
    }

    #[test]
    fn toml_candidates_auto_name_from_field_paths() {
        let toml = r#"
id = "101"
username = "jdoe"
[location]
city = "New York"
"#;

        let candidates = parameter_clickable_candidates(toml, false);
        assert!(candidates.iter().any(|candidate| {
            candidate.label == "id"
                && candidate.target == "101"
                && candidate.suggested_name.as_deref() == Some("id")
        }));
        assert!(candidates.iter().any(|candidate| {
            candidate.label == "location.city"
                && candidate.target == "New York"
                && candidate.suggested_name.as_deref() == Some("location_city")
        }));
    }

    #[test]
    fn full_mode_structured_content_requires_manual_parameter_names() {
        let json = r#"{"id":"101","username":"jdoe","email":"jdoe@example.com"}"#;
        let candidates = parameter_clickable_candidates(json, true);
        assert!(candidates.iter().any(|candidate| candidate.target == "101"));
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.suggested_name.is_none())
        );
    }

    #[test]
    fn full_mode_splits_assignment_into_individual_tokens() {
        let assignment = "password = test123";
        let candidates = parameter_clickable_candidates(assignment, true);
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.label == "password")
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.label == "test123")
        );
        assert!(
            !candidates
                .iter()
                .any(|candidate| candidate.label.contains('=')),
            "full mode should not emit combined assignment labels"
        );
    }

    #[test]
    fn unstructured_text_keeps_manual_parameter_naming() {
        let sql = "SELECT * FROM regprc.registration_transaction WHERE reg_id = '10004103';";
        let candidates = parameter_clickable_candidates(sql, false);
        assert!(!candidates.is_empty());
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.suggested_name.is_none())
        );
    }

    #[test]
    fn parameter_name_stage_focuses_first_incomplete_name() {
        let names = vec!["".to_owned(), "second_name".to_owned(), "".to_owned()];
        assert_eq!(first_parameter_name_issue_index(&names), 0);
    }

    #[test]
    fn parameter_name_stage_focuses_first_duplicate_name() {
        let names = vec![
            "first_name".to_owned(),
            "second_name".to_owned(),
            "first_name".to_owned(),
        ];
        assert_eq!(first_parameter_name_issue_index(&names), 2);
    }

    #[test]
    fn tag_search_suggestion_replaces_last_fragment() {
        assert_eq!(
            apply_tag_search_suggestion_to_query(":rust com", "command"),
            Some(":rust command".to_owned())
        );
        assert_eq!(
            apply_tag_search_suggestion_to_query(":rust ", "command"),
            Some(":rust command".to_owned())
        );
    }

    #[test]
    fn tag_search_suggestion_requires_tag_mode_query() {
        assert_eq!(apply_tag_search_suggestion_to_query("rust", "command"), None);
    }
}
