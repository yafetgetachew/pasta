use super::state::{CachedRowPresentation, SearchRequest, SearchResponse, TextInputState};
use crate::storage::{
    SEMANTIC_MIN_QUERY_CHARS, SEMANTIC_SOURCE_TEXT_LIMIT, SearchExecution, bounded_text_prefix,
    encode_f32_vec_base64, semantic_embedding,
};
use crate::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use toml::Value as TomlValue;

const TAG_SEARCH_AUTOCOMPLETE_LIMIT: usize = 6;
const SEARCH_SEMANTIC_DELAY_MS: u64 = 90;
const SEARCH_NEURAL_DELAY_MS: u64 = 320;
const SEARCH_NEURAL_MIN_QUERY_CHARS: usize = 5;

impl LauncherView {
    pub(crate) fn new(
        storage: Arc<ClipboardStorage>,
        font_family: SharedString,
        surface_alpha: f32,
        theme_mode: ThemeMode,
        syntax_highlighting: bool,
        pasta_brain_enabled: bool,
        search_request_tx: mpsc::Sender<SearchRequest>,
        search_generation_token: Arc<std::sync::atomic::AtomicU64>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            storage,
            font_family,
            surface_alpha,
            theme_mode,
            syntax_highlighting,
            pasta_brain_enabled,
            query_input_state: TextInputState::new(cx),
            info_editor_input_state: TextInputState::new(cx),
            tag_editor_input_state: TextInputState::new(cx),
            bowl_editor_input_state: TextInputState::new(cx),
            parameter_name_input_state: TextInputState::new(cx),
            parameter_fill_input_state: TextInputState::new(cx),
            pending_text_input_focus: None,
            results_scroll: UniformListScrollHandle::new(),
            search_request_tx,
            search_generation: 0,
            search_generation_token,
            latest_applied_search_execution: SearchExecution::Fast,
            query: String::new(),
            last_query_edit_at: None,
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
            bowl_editor_target_id: None,
            bowl_editor_input: String::new(),
            bowl_editor_select_all: false,
            bowl_editor_suggestions: Vec::new(),
            parameter_editor_target_id: None,
            parameter_editor_stage: ParameterEditorStage::SelectValue,
            parameter_editor_force_full: true,
            parameter_editor_selected_targets: Vec::new(),
            parameter_editor_name_inputs: Vec::new(),
            parameter_editor_name_focus_index: 0,
            parameter_editor_name_select_all: false,
            parameter_editor_split_tokens: HashSet::new(),
            parameter_fill_target_id: None,
            parameter_fill_values: Vec::new(),
            parameter_fill_focus_index: 0,
            parameter_fill_select_all: false,
            transform_menu_open: false,
            qr_preview: None,
            blur_close_armed: false,
            suppress_auto_hide: false,
            suppress_auto_hide_until: None,
            show_command_help: false,
            last_window_appearance: None,
        };
        view.begin_search_generation();
        view.request_search(SearchExecution::Fast);
        view
    }

    pub(crate) fn reset_for_show(&mut self) {
        self.query.clear();
        self.last_query_edit_at = None;
        self.tag_search_suggestions.clear();
        self.query_input_state.reset();
        self.info_editor_input_state.reset();
        self.tag_editor_input_state.reset();
        self.bowl_editor_input_state.reset();
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
        self.bowl_editor_target_id = None;
        self.bowl_editor_input.clear();
        self.bowl_editor_select_all = false;
        self.bowl_editor_suggestions.clear();
        self.parameter_editor_target_id = None;
        self.parameter_editor_stage = ParameterEditorStage::SelectValue;
        self.parameter_editor_force_full = true;
        self.parameter_editor_selected_targets.clear();
        self.parameter_editor_name_inputs.clear();
        self.parameter_editor_name_focus_index = 0;
        self.parameter_editor_name_select_all = false;
        self.parameter_editor_split_tokens.clear();
        self.parameter_fill_target_id = None;
        self.parameter_fill_values.clear();
        self.parameter_fill_focus_index = 0;
        self.parameter_fill_select_all = false;
        self.transform_menu_open = false;
        self.qr_preview = None;
        self.blur_close_armed = false;
        self.suppress_auto_hide = false;
        self.suppress_auto_hide_until = None;
        self.show_command_help = false;
        self.last_window_appearance = None;
        self.begin_search_generation();
        self.request_search(SearchExecution::Fast);
    }

    pub(crate) fn set_items(&mut self, items: Vec<ClipboardRecord>) {
        self.items = items;
        self.row_presentations = CachedRowPresentation::collect(&self.items);
        if self.selected_index >= self.items.len() {
            self.selected_index = 0;
        }
    }

    pub(crate) fn set_search_results(
        &mut self,
        items: Vec<ClipboardRecord>,
        row_presentations: Vec<CachedRowPresentation>,
    ) {
        self.items = items;
        self.row_presentations = row_presentations;
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
        let selection_changed = self.selected_index != 0;
        self.selected_index = 0;
        self.last_query_edit_at = Some(Instant::now());
        self.mark_selection_changed(cx);
        if selection_changed {
            self.reset_results_scroll_to_top();
        }
        self.refresh_tag_search_suggestions_async(cx);
        self.schedule_query_refresh();
        self.schedule_delayed_query_refresh(
            SearchExecution::Semantic,
            SEARCH_SEMANTIC_DELAY_MS,
            cx,
        );
        self.schedule_delayed_query_refresh(SearchExecution::Neural, SEARCH_NEURAL_DELAY_MS, cx);
        cx.notify();
    }

    fn refresh_tag_search_suggestions_async(&self, cx: &mut Context<Self>) {
        let storage = self.storage.clone();
        let query = self.query.clone();
        let background_query = query.clone();
        let expected_generation = self.search_generation;

        cx.spawn(async move |this, cx| {
            let suggestions = cx
                .background_executor()
                .spawn(async move {
                    storage.suggest_search_tokens(&background_query, TAG_SEARCH_AUTOCOMPLETE_LIMIT)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.search_generation != expected_generation || view.query != query {
                    return;
                }
                view.tag_search_suggestions = suggestions;
                cx.notify();
            });
        })
        .detach();
    }

    fn set_query_text(&mut self, query: String) {
        self.set_text_input_content(TextInputTarget::Query, query);
        let cursor = self.query.len();
        self.query_input_state.selected_range = cursor..cursor;
        self.query_input_state.selection_reversed = false;
        self.query_input_state.marked_range = None;
    }

    pub(crate) fn apply_tag_search_suggestion_index(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(suggestion) = self.tag_search_suggestions.get(index).cloned() else {
            return;
        };
        let Some(next_query) = apply_search_suggestion_to_query(&self.query, &suggestion) else {
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
        self.qr_preview = None;
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
        #[cfg(target_os = "linux")]
        {
            self.pending_exit = None;
            self.transition_from = 1.0;
            self.transition_alpha = 1.0;
            self.transition_target = 1.0;
            self.transition_started_at = Instant::now();
            self.transition_duration = Duration::ZERO;
        }

        #[cfg(not(target_os = "linux"))]
        {
            self.pending_exit = None;
            self.transition_from = 0.0;
            self.transition_alpha = self.transition_from;
            self.transition_target = 1.0;
            self.transition_started_at = Instant::now();
            self.transition_duration = Duration::from_millis(WINDOW_OPEN_DURATION_MS);
        }
    }

    pub(crate) fn begin_close_transition(&mut self, intent: LauncherExitIntent) {
        #[cfg(target_os = "linux")]
        {
            self.pending_exit = Some(intent);
            self.transition_from = 0.0;
            self.transition_alpha = 0.0;
            self.transition_target = 0.0;
            self.transition_started_at = Instant::now();
            self.transition_duration = Duration::ZERO;
        }

        #[cfg(not(target_os = "linux"))]
        {
            self.pending_exit = Some(intent);
            self.transition_from = self.transition_alpha.clamp(0.0, 1.0);
            self.transition_target = 0.0;
            self.transition_started_at = Instant::now();
            self.transition_duration = Duration::from_millis(WINDOW_CLOSE_DURATION_MS);
        }
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
        if !self.authenticate_secret_action("Reveal secret in Pasta", cx) {
            return;
        }

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

    pub(crate) fn refresh_items(&mut self, execution: SearchExecution) {
        let items = self
            .storage
            .search_items(
                &self.query,
                48,
                self.pasta_brain_enabled,
                execution,
                self.search_generation,
                None,
            )
            .unwrap_or_else(|_| Vec::new());
        self.set_items(items);
    }

    pub(crate) fn request_search(&mut self, execution: SearchExecution) {
        if self
            .search_request_tx
            .send(SearchRequest {
                query_generation: self.search_generation,
                query: self.query.clone(),
                pasta_brain: self.pasta_brain_enabled,
                execution,
            })
            .is_err()
        {
            // Fallback for environments where the worker thread is unavailable.
            self.refresh_items(execution);
        }
    }

    pub(crate) fn apply_search_response(&mut self, response: SearchResponse) -> bool {
        if response.query_generation != self.search_generation {
            return false;
        }
        if response.execution < self.latest_applied_search_execution {
            return false;
        }

        let previous_selected_id = self.items.get(self.selected_index).map(|item| item.id);
        let next_selected_id = response.items.get(self.selected_index).map(|item| item.id);
        self.set_search_results(response.items, response.row_presentations);
        self.latest_applied_search_execution = response.execution;
        if previous_selected_id != next_selected_id {
            self.selection_changed_at = Instant::now();
        }
        if self.selected_index == 0 {
            self.reset_results_scroll_to_top();
        }
        true
    }

    pub(crate) fn schedule_query_refresh(&mut self) {
        self.begin_search_generation();
        self.request_search(SearchExecution::Fast);
    }

    pub(crate) fn preferred_refresh_execution(&self) -> SearchExecution {
        if should_schedule_delayed_search(
            &self.query,
            self.pasta_brain_enabled,
            SearchExecution::Neural,
        ) {
            SearchExecution::Neural
        } else if should_schedule_delayed_search(
            &self.query,
            self.pasta_brain_enabled,
            SearchExecution::Semantic,
        ) {
            SearchExecution::Semantic
        } else {
            SearchExecution::Fast
        }
    }

    fn begin_search_generation(&mut self) {
        self.search_generation = self.search_generation.wrapping_add(1);
        self.search_generation_token
            .store(self.search_generation, Ordering::Release);
        self.latest_applied_search_execution = SearchExecution::Fast;
    }

    fn schedule_delayed_query_refresh(
        &self,
        execution: SearchExecution,
        delay_ms: u64,
        cx: &mut Context<Self>,
    ) {
        if !should_schedule_delayed_search(&self.query, self.pasta_brain_enabled, execution) {
            return;
        }

        let expected_generation = self.search_generation;
        let expected_query = self.query.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay_ms))
                .await;
            let _ = this.update(cx, |view, _cx| {
                if view.search_generation != expected_generation || view.query != expected_query {
                    return;
                }
                view.request_search(execution);
            });
        })
        .detach();
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
        // On Wayland, the GPUI window must stay alive to serve paste requests.
        // Since Pasta destroys the window on hide, also write via wl-clipboard-rs
        // which forks a background process to serve the data independently.
        #[cfg(target_os = "linux")]
        write_clipboard_text(&item.content);
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
                self.refresh_items(self.preferred_refresh_execution());
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
                self.reset_secret_reveal_state();
                self.refresh_after_selected_item_update(item_id);
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

    pub(crate) fn unmark_selected_item_as_secret(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(self.selected_index).cloned() else {
            return;
        };
        if item.item_type != ClipboardItemType::Password {
            show_macos_notification("Pasta", "Item is already unprotected.");
            return;
        }
        if !self.authenticate_secret_action("Remove secret protection in Pasta", cx) {
            return;
        }

        match self.storage.unmark_item_as_secret(item.id) {
            Ok(true) => {
                self.reset_secret_reveal_state();
                self.refresh_after_selected_item_update(item.id);
                show_macos_notification("Pasta", "Secret protection removed.");
                cx.notify();
            }
            Ok(false) => {
                show_macos_notification("Pasta", "Item is already unprotected.");
            }
            Err(err) => {
                eprintln!("warning: failed to unmark item as secret: {err}");
                show_macos_notification("Pasta", "Failed to remove secret protection.");
            }
        }
    }

    pub(crate) fn toggle_selected_item_secret_state(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(self.selected_index) else {
            return;
        };

        if item.item_type == ClipboardItemType::Password {
            self.unmark_selected_item_as_secret(cx);
        } else {
            self.mark_selected_item_as_secret(cx);
        }
    }

    fn reset_secret_reveal_state(&mut self) {
        self.revealed_secret_id = None;
        self.reveal_until = None;
        self.last_reveal_second_bucket = None;
    }

    fn authenticate_secret_action(&mut self, reason: &str, cx: &mut Context<Self>) -> bool {
        self.suppress_auto_hide = true;
        let authenticated = authenticate_with_touch_id(reason);
        self.suppress_auto_hide = false;
        self.suppress_auto_hide_until = Some(Instant::now() + Duration::from_millis(250));
        if !authenticated {
            return false;
        }

        cx.activate(true);
        true
    }

    fn refresh_after_selected_item_update(&mut self, item_id: i64) {
        let previous_index = self.selected_index;
        self.refresh_items(self.preferred_refresh_execution());
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
                self.refresh_items(self.preferred_refresh_execution());
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

    pub(crate) fn start_bowl_editor_for_selected(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(self.selected_index).cloned() else {
            return;
        };

        self.info_editor_target_id = None;
        self.info_editor_input.clear();
        self.info_editor_input_state.reset();
        self.info_editor_select_all = false;
        self.tag_editor_target_id = None;
        self.tag_editor_input.clear();
        self.tag_editor_input_state.reset();
        self.tag_editor_select_all = false;
        self.tag_editor_mode = TagEditorMode::Add;
        self.bowl_editor_target_id = Some(item.id);
        self.bowl_editor_input = bowl_name_from_tags(&item.tags).unwrap_or_default();
        self.bowl_editor_input_state.reset();
        if self.bowl_editor_input.is_empty() {
            self.bowl_editor_input_state.selected_range = 0..0;
            self.bowl_editor_select_all = false;
        } else {
            self.bowl_editor_input_state.selected_range = 0..self.bowl_editor_input.len();
            self.bowl_editor_select_all = true;
        }
        self.bowl_editor_input_state.selection_reversed = false;
        self.bowl_editor_input_state.marked_range = None;
        self.bowl_editor_suggestions = self.storage.suggest_bowl_names(&self.bowl_editor_input, 6);
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
        self.queue_text_input_focus(TextInputTarget::BowlEditor);
        cx.notify();
    }

    pub(crate) fn commit_bowl_editor(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.bowl_editor_target_id else {
            return;
        };

        let parsed = parse_custom_tags_input(&self.bowl_editor_input);
        let requested_bowl = match parsed.as_slice() {
            [] => None,
            [bowl] => Some(bowl.as_str()),
            _ => {
                show_macos_notification("Pasta", "Enter a single bowl name.");
                return;
            }
        };

        match self.storage.set_item_bowl(item_id, requested_bowl) {
            Ok(changed) => {
                let previous_index = self.selected_index;
                self.refresh_items(self.preferred_refresh_execution());
                if let Some(ix) = self.items.iter().position(|entry| entry.id == item_id) {
                    self.selected_index = ix;
                    self.selection_changed_at = Instant::now();
                    self.results_scroll
                        .scroll_to_item(ix, ScrollStrategy::Center);
                } else if !self.items.is_empty() {
                    self.selected_index = previous_index.min(self.items.len().saturating_sub(1));
                    self.selection_changed_at = Instant::now();
                    self.results_scroll
                        .scroll_to_item(self.selected_index, ScrollStrategy::Center);
                }

                self.bowl_editor_target_id = None;
                self.bowl_editor_input.clear();
                self.bowl_editor_input_state.reset();
                self.bowl_editor_select_all = false;
                self.bowl_editor_suggestions.clear();
                self.queue_text_input_focus(TextInputTarget::Query);
                if changed {
                    show_macos_notification(
                        "Pasta",
                        if requested_bowl.is_some() {
                            "Bowl saved."
                        } else {
                            "Bowl cleared."
                        },
                    );
                } else {
                    show_macos_notification(
                        "Pasta",
                        if requested_bowl.is_some() {
                            "Bowl unchanged."
                        } else {
                            "No bowl to clear."
                        },
                    );
                }
                cx.notify();
            }
            Err(err) => {
                eprintln!("warning: failed to update bowl: {err}");
                show_macos_notification("Pasta", "Failed to update bowl.");
            }
        }
    }

    pub(crate) fn cancel_bowl_editor(&mut self, cx: &mut Context<Self>) {
        self.bowl_editor_target_id = None;
        self.bowl_editor_input.clear();
        self.bowl_editor_input_state.reset();
        self.bowl_editor_select_all = false;
        self.bowl_editor_suggestions.clear();
        self.queue_text_input_focus(TextInputTarget::Query);
        cx.notify();
    }

    pub(crate) fn remove_bowl_from_selected(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.items.get(self.selected_index).map(|item| item.id) else {
            return;
        };

        match self.storage.set_item_bowl(item_id, None) {
            Ok(changed) => {
                if changed {
                    let previous_index = self.selected_index;
                    self.refresh_items(self.preferred_refresh_execution());
                    if let Some(ix) = self.items.iter().position(|entry| entry.id == item_id) {
                        self.selected_index = ix;
                        self.selection_changed_at = Instant::now();
                        self.results_scroll
                            .scroll_to_item(ix, ScrollStrategy::Center);
                    } else if !self.items.is_empty() {
                        self.selected_index =
                            previous_index.min(self.items.len().saturating_sub(1));
                        self.selection_changed_at = Instant::now();
                        self.results_scroll
                            .scroll_to_item(self.selected_index, ScrollStrategy::Center);
                    }
                    show_macos_notification("Pasta", "Removed from bowl.");
                    cx.notify();
                } else {
                    show_macos_notification("Pasta", "Snippet is not in a bowl.");
                }
            }
            Err(err) => {
                eprintln!("warning: failed to remove bowl: {err}");
                show_macos_notification("Pasta", "Failed to remove bowl.");
            }
        }
    }

    pub(crate) fn export_bowl_from_query(&mut self, cx: &mut Context<Self>) {
        let SearchQuery::ExportBowl { bowl_query } = parse_search_query(&self.query) else {
            return;
        };
        self.export_bowl_named(&bowl_query, cx);
    }

    fn export_bowl_named(&mut self, bowl_name: &str, cx: &mut Context<Self>) {
        let bowl_name = bowl_name.trim();
        if bowl_name.is_empty() {
            show_macos_notification(
                "Pasta",
                "Export denied: enter a bowl name after :e so Pasta knows which bowl to export.",
            );
            return;
        }

        let items = self.storage.items_in_bowl(bowl_name);
        if items.is_empty() {
            show_macos_notification(
                "Pasta",
                &format!(
                    "Export denied: bowl {bowl_name} has no snippets, so there is nothing to export."
                ),
            );
            return;
        }

        if let Some(item) = items.iter().find(|item| {
            item.item_type != ClipboardItemType::Password && item.description.trim().is_empty()
        }) {
            show_macos_notification(
                "Pasta",
                &format!(
                    "Export denied: snippet #{} doesn't have a description. Every non-secret snippet in a bowl needs a description before export.",
                    item.id
                ),
            );
            return;
        }

        let bundle = build_bowl_export_bundle(bowl_name, &items, &self.storage);
        self.suppress_auto_hide = true;
        let Some(path) = choose_bowl_export_path(
            "Export Pasta bowl",
            &suggested_bowl_export_filename(&bundle.bowl),
        ) else {
            self.suppress_auto_hide = false;
            self.suppress_auto_hide_until = Some(Instant::now() + Duration::from_millis(350));
            cx.activate(true);
            cx.notify();
            return;
        };

        self.suppress_auto_hide = false;
        self.suppress_auto_hide_until = Some(Instant::now() + Duration::from_millis(350));
        cx.activate(true);

        let yaml = match serde_yaml::to_string(&bundle) {
            Ok(yaml) => yaml,
            Err(err) => {
                eprintln!("warning: failed to serialize bowl export: {err}");
                show_macos_notification("Pasta", "Failed to export bowl.");
                return;
            }
        };

        if let Err(err) = std::fs::write(&path, yaml) {
            eprintln!("warning: failed to write bowl export file: {err}");
            show_macos_notification("Pasta", "Failed to export bowl.");
            return;
        }

        let excluded_secret_count = items
            .iter()
            .filter(|item| item.item_type == ClipboardItemType::Password)
            .count();
        show_macos_notification(
            "Pasta",
            &if excluded_secret_count == 0 {
                format!(
                    "Exported {} snippets from {}.",
                    bundle.items.len(),
                    bundle.bowl
                )
            } else {
                format!(
                    "Exported {} entries from {}. {} secret {} redacted.",
                    bundle.items.len(),
                    bundle.bowl,
                    excluded_secret_count,
                    if excluded_secret_count == 1 {
                        "was"
                    } else {
                        "were"
                    }
                )
            },
        );
    }

    pub(crate) fn import_bowl_from_picker(&mut self, cx: &mut Context<Self>) {
        self.suppress_auto_hide = true;
        let Some(path) = choose_bowl_import_path("Import Pasta bowl") else {
            self.suppress_auto_hide = false;
            self.suppress_auto_hide_until = Some(Instant::now() + Duration::from_millis(350));
            cx.activate(true);
            self.queue_text_input_focus(TextInputTarget::Query);
            cx.notify();
            return;
        };
        self.suppress_auto_hide = false;
        self.suppress_auto_hide_until = Some(Instant::now() + Duration::from_millis(350));
        cx.activate(true);

        let yaml = match std::fs::read_to_string(&path) {
            Ok(yaml) => yaml,
            Err(err) => {
                eprintln!("warning: failed to read bowl import file: {err}");
                show_macos_notification("Pasta", "Failed to read bowl file.");
                self.queue_text_input_focus(TextInputTarget::Query);
                cx.notify();
                return;
            }
        };

        let bundle = match serde_yaml::from_str::<BowlExportBundle>(&yaml) {
            Ok(bundle) => bundle,
            Err(err) => {
                eprintln!("warning: failed to parse bowl import file: {err}");
                show_macos_notification("Pasta", "Failed to parse bowl file.");
                self.queue_text_input_focus(TextInputTarget::Query);
                cx.notify();
                return;
            }
        };

        match self.storage.import_bowl_bundle(&bundle) {
            Ok(summary) => {
                self.set_query_text(format!(":b {}", summary.bowl));
                self.queue_text_input_focus(TextInputTarget::Query);
                self.query_did_change(cx);
                show_macos_notification(
                    "Pasta",
                    &format!(
                        "Imported {} snippets into {}.",
                        summary.imported_count, summary.bowl
                    ),
                );
            }
            Err(err) => {
                eprintln!("warning: failed to import bowl: {err}");
                show_macos_notification("Pasta", "Failed to import bowl.");
                self.queue_text_input_focus(TextInputTarget::Query);
                cx.notify();
            }
        }
    }

    pub(crate) fn add_custom_tags_to_selected(&mut self, cx: &mut Context<Self>) {
        let Some(item_id) = self.items.get(self.selected_index).map(|item| item.id) else {
            return;
        };
        self.info_editor_target_id = None;
        self.info_editor_input.clear();
        self.info_editor_input_state.reset();
        self.info_editor_select_all = false;
        self.bowl_editor_target_id = None;
        self.bowl_editor_input.clear();
        self.bowl_editor_input_state.reset();
        self.bowl_editor_select_all = false;
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
        self.bowl_editor_target_id = None;
        self.bowl_editor_input.clear();
        self.bowl_editor_input_state.reset();
        self.bowl_editor_select_all = false;
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
                self.refresh_items(self.preferred_refresh_execution());
                if let Some(ix) = self.items.iter().position(|entry| entry.id == item_id) {
                    self.selected_index = ix;
                    self.selection_changed_at = Instant::now();
                    self.results_scroll
                        .scroll_to_item(ix, ScrollStrategy::Center);
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
        self.parameter_editor_split_tokens.clear();
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
        self.parameter_editor_split_tokens.clear();
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
        self.parameter_editor_split_tokens.clear();
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
            self.refresh_items(self.preferred_refresh_execution());
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
        let raw_candidates =
            parameter_clickable_candidates(&content, self.parameter_editor_force_full);

        if additive {
            // Cmd+click: if the clicked token is sub-splittable, toggle its expansion.
            let Some(raw_candidate) = raw_candidates.get(range_index) else {
                // range_index maps into expanded list — resolve from there.
                let expanded = expand_candidates_with_splits(
                    raw_candidates,
                    &self.parameter_editor_split_tokens,
                );
                if let Some(candidate) = expanded.get(range_index) {
                    self.toggle_parameter_target(
                        &candidate.target,
                        candidate.suggested_name.as_deref(),
                    );
                }
                cx.notify();
                return;
            };
            if token_is_sub_splittable(&raw_candidate.target)
                && !self
                    .parameter_editor_split_tokens
                    .contains(&raw_candidate.target)
            {
                // First Cmd+click: expand this token into sub-tokens.
                self.parameter_editor_split_tokens
                    .insert(raw_candidate.target.clone());
                cx.notify();
                return;
            }
            // Token is already expanded or not splittable — fall through to toggle.
        }

        // Use expanded candidates for resolving the actual index.
        let expanded =
            expand_candidates_with_splits(raw_candidates, &self.parameter_editor_split_tokens);
        let Some(candidate) = expanded.get(range_index) else {
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
        if let Some(active_value) = self
            .parameter_fill_values
            .get(self.parameter_fill_focus_index)
        {
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
        #[cfg(target_os = "linux")]
        write_clipboard_text(&rendered);
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

    pub(crate) fn handle_bowl_editor_keystroke(
        &mut self,
        event: &KeystrokeEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();

        match key {
            "escape" | "esc" => {
                self.cancel_bowl_editor(cx);
                return;
            }
            "enter" | "return" => {
                self.commit_bowl_editor(cx);
                return;
            }
            "tab" if !event.keystroke.modifiers.modified() => {
                if let Some(suggestion) = self.bowl_editor_suggestions.first().cloned() {
                    self.bowl_editor_input = suggestion;
                    let len = self.bowl_editor_input.len();
                    self.bowl_editor_input_state.selected_range = len..len;
                    self.bowl_editor_input_state.selection_reversed = false;
                    self.bowl_editor_input_state.marked_range = None;
                    self.bowl_editor_suggestions =
                        self.storage.suggest_bowl_names(&self.bowl_editor_input, 6);
                    cx.notify();
                }
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

        if matches!(action, TransformAction::QrCode) {
            match qr_encode_matrix(&item.content) {
                Ok(matrix) => {
                    self.qr_preview = Some((item.id, matrix));
                    self.transform_menu_open = false;
                    cx.notify();
                }
                Err(err) => {
                    show_macos_notification("Pasta", &err);
                }
            }
            return;
        }

        let outcome = match action {
            TransformAction::ShellQuote => Ok((
                shell_quote_escape(&item.content),
                "Shell-quoted to clipboard.",
            )),
            TransformAction::JsonEncode => json_encode_transform(&item.content),
            TransformAction::JsonDecode => json_decode_transform(&item.content),
            TransformAction::JsonPretty => json_pretty_transform(&item.content),
            TransformAction::JsonMinify => json_minify_transform(&item.content),
            TransformAction::UrlEncode => url_encode_transform(&item.content),
            TransformAction::UrlDecode => url_decode_transform(&item.content),
            TransformAction::Base64Encode => base64_encode_transform(&item.content),
            TransformAction::Base64Decode => base64_decode_transform(&item.content),
            TransformAction::JwtDecode => jwt_decode_transform(&item.content),
            TransformAction::EpochDecode => epoch_decode_transform(&item.content),
            TransformAction::Sha256Hash => sha256_hash_transform(&item.content),
            TransformAction::ContentStats => content_stats_transform(&item.content),
            TransformAction::PublicCertPemInfo => public_cert_pem_info_transform(&item.content),
            TransformAction::QrCode => unreachable!("handled above"),
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
        #[cfg(target_os = "linux")]
        write_clipboard_text(&transformed);

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
        self.refresh_items(self.preferred_refresh_execution());
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

        // On macOS, Cmd (modifiers.platform) is the action modifier.
        // On Linux, Ctrl (modifiers.control) is the standard app shortcut
        // modifier — GPUI maps modifiers.platform to Super/Meta on Linux.
        let action_mod = if cfg!(target_os = "macos") {
            modifiers.platform && !modifiers.control
        } else {
            modifiers.control && !modifiers.platform
        };

        let command_navigation =
            action_mod && !modifiers.shift && !modifiers.alt && !modifiers.function;

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

        if self.bowl_editor_target_id.is_some() {
            self.handle_bowl_editor_keystroke(event, cx);
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
            if self.qr_preview.is_some() {
                self.qr_preview = None;
                cx.notify();
                return;
            }
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
                if matches!(
                    parse_search_query(&self.query),
                    SearchQuery::ExportBowl { .. }
                ) {
                    self.export_bowl_from_query(cx);
                    return;
                }
                self.copy_selected_to_clipboard(cx);
                return;
            }
            "delete" | "forwarddelete" => {
                self.delete_selected_item(cx);
                return;
            }
            "d" if action_mod && !modifiers.alt && !modifiers.function => {
                self.delete_selected_item(cx);
                return;
            }
            "r" if action_mod && !modifiers.alt && !modifiers.function => {
                self.reveal_and_copy_selected_secret(cx);
                return;
            }
            "s" if action_mod && modifiers.shift && !modifiers.alt && !modifiers.function => {
                self.toggle_selected_item_secret_state(cx);
                return;
            }
            "h" if action_mod && !modifiers.alt && !modifiers.function => {
                self.show_command_help = !self.show_command_help;
                cx.notify();
                return;
            }
            "t" if action_mod && modifiers.shift && !modifiers.alt && !modifiers.function => {
                self.remove_custom_tags_from_selected(cx);
                return;
            }
            "t" if action_mod && !modifiers.shift && !modifiers.alt && !modifiers.function => {
                self.add_custom_tags_to_selected(cx);
                return;
            }
            "b" if action_mod && modifiers.shift && !modifiers.alt && !modifiers.function => {
                self.remove_bowl_from_selected(cx);
                return;
            }
            "b" if action_mod && !modifiers.shift && modifiers.alt && !modifiers.function => {
                self.import_bowl_from_picker(cx);
                return;
            }
            "b" if action_mod && !modifiers.shift && !modifiers.alt && !modifiers.function => {
                self.start_bowl_editor_for_selected(cx);
                return;
            }
            "p" if action_mod && !modifiers.shift && !modifiers.alt && !modifiers.function => {
                self.start_parameter_editor_for_selected(cx);
                return;
            }
            "i" if action_mod && !modifiers.shift && !modifiers.alt && !modifiers.function => {
                self.start_info_editor_for_selected(cx);
                return;
            }
            "q" if action_mod && !modifiers.alt && !modifiers.function => {
                self.begin_close_transition(LauncherExitIntent::Hide);
                cx.notify();
                return;
            }
            "backspace" if action_mod && !modifiers.alt && !modifiers.function => {
                self.delete_selected_item(cx);
                return;
            }
            _ => {}
        }
    }
}

fn is_parameter_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':')
}

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

#[derive(Clone)]
pub(super) struct ParameterClickableCandidate {
    pub(super) target: String,
    pub(super) label: String,
    pub(super) suggested_name: Option<String>,
}

fn is_sub_split_delimiter(ch: char) -> bool {
    matches!(ch, '/' | '.' | ':')
}

fn token_is_sub_splittable(target: &str) -> bool {
    target.chars().any(is_sub_split_delimiter)
        && target
            .chars()
            .filter(|ch| is_sub_split_delimiter(*ch))
            .count()
            <= 8
}

fn sub_split_token(target: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();

    for ch in target.chars() {
        if is_sub_split_delimiter(ch) {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

pub(super) fn expand_candidates_with_splits(
    candidates: Vec<ParameterClickableCandidate>,
    split_tokens: &HashSet<String>,
) -> Vec<ParameterClickableCandidate> {
    if split_tokens.is_empty() {
        return candidates;
    }

    let mut expanded = Vec::new();
    for candidate in candidates {
        if split_tokens.contains(&candidate.target) && token_is_sub_splittable(&candidate.target) {
            let sub_parts = sub_split_token(&candidate.target);
            for part in sub_parts {
                if part.is_empty() {
                    continue;
                }
                let label = preview_for_parameter_candidate(&part, 28);
                expanded.push(ParameterClickableCandidate {
                    target: part,
                    label,
                    suggested_name: None,
                });
            }
        } else {
            expanded.push(candidate);
        }
    }
    expanded
}

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

pub(super) fn has_structured_parameter_candidates(content: &str) -> bool {
    !parameter_structured_candidates(content).is_empty()
}

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

fn parameter_toml_scalar_candidates(content: &str) -> Vec<ParameterClickableCandidate> {
    let Ok(toml_value) = toml::from_str::<TomlValue>(content) else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    collect_toml_parameter_candidates(&toml_value, String::new(), &mut candidates, 0);
    candidates
}

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

fn apply_search_suggestion_to_query(query: &str, suggestion: &str) -> Option<String> {
    let normalized_suggestion = suggestion.trim();
    if normalized_suggestion.is_empty() {
        return None;
    }

    if let Some(effective_query) = raw_tag_search_effective_query(query) {
        let normalized_suggestion = normalized_suggestion.to_ascii_lowercase();
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
    } else {
        match parse_search_query(query) {
            SearchQuery::Bowl { .. } => Some(format!(":b {normalized_suggestion}")),
            SearchQuery::ExportBowl { .. } => Some(format!(":e {normalized_suggestion}")),
            SearchQuery::Default { .. } => None,
            SearchQuery::TagOnly { .. } => None,
        }
    }
}

fn should_schedule_delayed_search(
    query: &str,
    pasta_brain_enabled: bool,
    execution: SearchExecution,
) -> bool {
    match parse_search_query(query) {
        SearchQuery::Default { effective_query } => {
            if effective_query.is_empty() {
                return false;
            }
            let char_count = effective_query.chars().count();
            match execution {
                SearchExecution::Fast => true,
                SearchExecution::Semantic => char_count >= SEMANTIC_MIN_QUERY_CHARS,
                SearchExecution::Neural => {
                    pasta_brain_enabled && char_count >= SEARCH_NEURAL_MIN_QUERY_CHARS
                }
            }
        }
        SearchQuery::TagOnly { .. } | SearchQuery::Bowl { .. } | SearchQuery::ExportBowl { .. } => {
            false
        }
    }
}

fn raw_tag_search_effective_query(query: &str) -> Option<&str> {
    let trimmed_start = query.trim_start();
    let effective_query = trimmed_start.strip_prefix(':')?.trim_start();
    let trimmed_end = effective_query.trim_end();
    if trimmed_end == "b"
        || trimmed_end == "e"
        || trimmed_end.starts_with("b ")
        || trimmed_end.starts_with("e ")
    {
        return None;
    }

    Some(effective_query)
}

fn build_bowl_export_bundle(
    bowl_name: &str,
    items: &[ClipboardRecord],
    storage: &ClipboardStorage,
) -> BowlExportBundle {
    let bundle_bowl = items
        .iter()
        .find_map(|item| bowl_name_from_tags(&item.tags))
        .unwrap_or_else(|| bowl_name.trim().to_ascii_uppercase());

    BowlExportBundle {
        kind: "pasta-bowl".to_owned(),
        version: 1,
        bowl: bundle_bowl,
        exported_at: chrono::Utc::now().to_rfc3339(),
        items: items
            .iter()
            .map(|item| build_bowl_export_item(item, Some(storage)))
            .collect(),
    }
}

fn build_bowl_export_item(
    item: &ClipboardRecord,
    storage: Option<&ClipboardStorage>,
) -> BowlExportItem {
    if item.item_type == ClipboardItemType::Password {
        return BowlExportItem {
            item_type: item.item_type.as_str().to_owned(),
            content: String::new(),
            description: "Excluded: secrets are never exported".to_owned(),
            tags: Vec::new(),
            parameters: Vec::new(),
            hash_embedding: None,
            neural_embedding: None,
            embedding_model: None,
        };
    }

    let (hash_embedding, neural_embedding, embedding_model) = if let Some(storage) = storage {
        let seed_terms = tags_without_bowl(&item.tags);
        let hash_vec = semantic_embedding(
            bounded_text_prefix(&item.content, SEMANTIC_SOURCE_TEXT_LIMIT),
            &seed_terms,
        );
        let hash_emb = Some(encode_f32_vec_base64(&hash_vec));

        let neural_embedder = storage.get_neural_embedder();
        let (neural_emb, model) = if let Some(embedder) = neural_embedder.as_ref() {
            let neural_vec = embedder.embed(
                bounded_text_prefix(&item.content, SEMANTIC_SOURCE_TEXT_LIMIT),
                &seed_terms,
            );
            (
                Some(encode_f32_vec_base64(&neural_vec)),
                Some("all-MiniLM-L6-v2".to_owned()),
            )
        } else {
            (None, None)
        };
        (hash_emb, neural_emb, model)
    } else {
        (None, None, None)
    };

    BowlExportItem {
        item_type: item.item_type.as_str().to_owned(),
        content: build_bowl_export_template_content(&item.content, &item.parameters),
        description: item.description.clone(),
        tags: tags_without_bowl(&item.tags),
        parameters: item
            .parameters
            .iter()
            .map(|parameter| BowlExportParameter {
                name: parameter.name.clone(),
                default_value: export_parameter_default_value(parameter),
            })
            .collect(),
        hash_embedding,
        neural_embedding,
        embedding_model,
    }
}

fn build_bowl_export_template_content(content: &str, parameters: &[ClipboardParameter]) -> String {
    if parameters.is_empty() {
        return content.to_owned();
    }

    let mut ordered = parameters.to_vec();
    ordered.sort_unstable_by(|left, right| right.target.len().cmp(&left.target.len()));

    let mut output = content.to_owned();
    let mut replacements = Vec::new();
    for (idx, parameter) in ordered.iter().enumerate() {
        if parameter.target.is_empty() {
            continue;
        }

        let mut marker = format!("\u{001F}PASTA_BOWL_EXPORT_{idx}\u{001E}");
        while output.contains(&marker) {
            marker.push('_');
        }
        output = output.replace(&parameter.target, &marker);
        replacements.push((marker, format!("{{{{{}}}}}", parameter.name.trim())));
    }

    for (marker, replacement) in replacements {
        output = output.replace(&marker, &replacement);
    }

    output
}

fn export_parameter_default_value(parameter: &ClipboardParameter) -> String {
    let placeholder = format!("{{{{{}}}}}", parameter.name.trim());
    if parameter.target.trim() == placeholder {
        String::new()
    } else {
        parameter.target.clone()
    }
}

fn suggested_bowl_export_filename(bowl_name: &str) -> String {
    let sanitized: String = bowl_name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let sanitized = sanitized.trim_matches('-');
    if sanitized.is_empty() {
        "pasta-bowl.yaml".to_owned()
    } else {
        format!("{sanitized}.yaml")
    }
}

fn is_parameter_key_token(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 80
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

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

#[cfg(test)]
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
            apply_search_suggestion_to_query(":rust com", "command"),
            Some(":rust command".to_owned())
        );
        assert_eq!(
            apply_search_suggestion_to_query(":rust ", "command"),
            Some(":rust command".to_owned())
        );
    }

    #[test]
    fn tag_search_suggestion_requires_tag_mode_query() {
        assert_eq!(apply_search_suggestion_to_query("rust", "command"), None);
    }

    #[test]
    fn search_suggestion_updates_bowl_queries() {
        assert_eq!(
            apply_search_suggestion_to_query(":b ops", "OPS"),
            Some(":b OPS".to_owned())
        );
        assert_eq!(
            apply_search_suggestion_to_query(":e op", "OPS"),
            Some(":e OPS".to_owned())
        );
    }

    #[test]
    fn delayed_search_stages_gate_by_query_mode_and_length() {
        assert!(!should_schedule_delayed_search(
            "",
            true,
            SearchExecution::Semantic
        ));
        assert!(!should_schedule_delayed_search(
            "ya",
            true,
            SearchExecution::Semantic
        ));
        assert!(should_schedule_delayed_search(
            "yaf",
            true,
            SearchExecution::Semantic
        ));
        assert!(!should_schedule_delayed_search(
            "yaf",
            true,
            SearchExecution::Neural
        ));
        assert!(should_schedule_delayed_search(
            "yafet",
            true,
            SearchExecution::Neural
        ));
        assert!(!should_schedule_delayed_search(
            "yafet",
            false,
            SearchExecution::Neural
        ));
        assert!(!should_schedule_delayed_search(
            ":ya",
            true,
            SearchExecution::Semantic
        ));
        assert!(!should_schedule_delayed_search(
            ":b ops",
            true,
            SearchExecution::Semantic
        ));
        assert!(!should_schedule_delayed_search(
            ":e ops",
            true,
            SearchExecution::Neural
        ));
    }

    #[test]
    fn bowl_export_uses_template_content_and_default_values() {
        let record = ClipboardRecord {
            id: 9,
            item_type: ClipboardItemType::Command,
            content: "kubectl logs -f deployment/api -n default --tail=100".to_owned(),
            description: "Stream logs from a deployment".to_owned(),
            tags: vec![
                "k8s".to_owned(),
                "logs".to_owned(),
                "BOWL:K8S-OPS".to_owned(),
            ],
            parameters: vec![
                ClipboardParameter {
                    name: "deployment".to_owned(),
                    target: "api".to_owned(),
                },
                ClipboardParameter {
                    name: "namespace".to_owned(),
                    target: "default".to_owned(),
                },
                ClipboardParameter {
                    name: "lines".to_owned(),
                    target: "100".to_owned(),
                },
            ],
            created_at: "2026-03-29T00:39:00Z".to_owned(),
        };

        let item = build_bowl_export_item(&record, None);
        assert_eq!(
            item.content,
            "kubectl logs -f deployment/{{deployment}} -n {{namespace}} --tail={{lines}}"
        );
        assert_eq!(
            item.parameters,
            vec![
                BowlExportParameter {
                    name: "deployment".to_owned(),
                    default_value: "api".to_owned(),
                },
                BowlExportParameter {
                    name: "namespace".to_owned(),
                    default_value: "default".to_owned(),
                },
                BowlExportParameter {
                    name: "lines".to_owned(),
                    default_value: "100".to_owned(),
                },
            ]
        );
        assert_eq!(item.tags, vec!["k8s".to_owned(), "logs".to_owned()]);
    }

    #[test]
    fn bowl_export_redacts_password_entries() {
        let record = ClipboardRecord {
            id: 11,
            item_type: ClipboardItemType::Password,
            content: "super-secret".to_owned(),
            description: "root cluster token".to_owned(),
            tags: vec!["secret".to_owned(), "BOWL:K8S-OPS".to_owned()],
            parameters: Vec::new(),
            created_at: "2026-03-29T00:39:00Z".to_owned(),
        };

        let item = build_bowl_export_item(&record, None);
        assert_eq!(item.item_type, "password");
        assert_eq!(item.content, "");
        assert_eq!(item.description, "Excluded: secrets are never exported");
        assert!(item.tags.is_empty());
        assert!(item.parameters.is_empty());
    }
}
