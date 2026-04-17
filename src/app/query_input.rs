#[cfg(target_os = "macos")]
use super::state::TextInputState;
#[cfg(target_os = "macos")]
use crate::*;
#[cfg(target_os = "macos")]
use unicode_segmentation::UnicodeSegmentation;

#[cfg(target_os = "macos")]
pub(super) struct TextInputElement {
    pub(super) input: Entity<LauncherView>,
    pub(super) target: TextInputTarget,
    pub(super) placeholder: SharedString,
    pub(super) palette: Palette,
    pub(super) enabled: bool,
}

#[cfg(target_os = "macos")]
pub(super) struct TextInputPrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

#[cfg(target_os = "macos")]
impl TextInputElement {
    pub(super) fn new(
        input: Entity<LauncherView>,
        target: TextInputTarget,
        placeholder: impl Into<SharedString>,
        palette: Palette,
        enabled: bool,
    ) -> Self {
        Self {
            input,
            target,
            placeholder: placeholder.into(),
            palette,
            enabled,
        }
    }
}

#[cfg(target_os = "macos")]
impl IntoElement for TextInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(target_os = "macos")]
impl GpuiElement for TextInputElement {
    type RequestLayoutState = ();
    type PrepaintState = TextInputPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.text_input_content(self.target).to_owned();
        let input_state = input.text_input_state(self.target);
        let selected_range = clamp_text_range(&content, &input_state.selected_range);
        let marked_range = input_state
            .marked_range
            .as_ref()
            .map(|range| clamp_text_range(&content, range));
        let cursor = clamp_text_offset(&content, input_state.cursor_offset());
        let style = window.text_style();

        let (display_text, text_color): (SharedString, gpui::Hsla) = if content.is_empty() {
            (
                self.placeholder.clone(),
                self.palette.query_placeholder.into(),
            )
        } else {
            (content.clone().into(), self.palette.query_active.into())
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if !content.is_empty() {
            if let Some(marked_range) = marked_range.as_ref() {
                vec![
                    TextRun {
                        len: marked_range.start,
                        ..run.clone()
                    },
                    TextRun {
                        len: marked_range.end - marked_range.start,
                        underline: Some(UnderlineStyle {
                            color: Some(run.color),
                            thickness: px(1.0),
                            wavy: false,
                        }),
                        ..run.clone()
                    },
                    TextRun {
                        len: display_text.len() - marked_range.end,
                        ..run
                    },
                ]
                .into_iter()
                .filter(|run| run.len > 0)
                .collect()
            } else {
                vec![run]
            }
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let cursor_pos = line.x_for_index(cursor);
        let (selection, cursor) = if selected_range.is_empty() || content.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(2.0), bounds.bottom() - bounds.top()),
                    ),
                    self.palette.selected_border,
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    self.palette.selected_bg,
                )),
                None,
            )
        };

        TextInputPrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).text_input_focus_handle(self.target);
        if self.enabled {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        }

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let Some(line) = prepaint.line.take() else {
            // Prepaint should always populate `line`; if it didn't, bail out rather
            // than aborting the process like an unwrap would.
            return;
        };
        if let Err(err) = line.paint(bounds.origin, window.line_height(), window, cx) {
            eprintln!("warning: query_input paint failed: {err}");
        }

        if self.enabled
            && focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            let input_state = input.text_input_state_mut(self.target);
            input_state.last_layout = Some(line);
            input_state.last_bounds = Some(bounds);
        });
    }
}

#[cfg(target_os = "macos")]
fn text_offset_from_utf16(text: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;

    for ch in text.chars() {
        if utf16_count >= offset {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_offset += ch.len_utf8();
    }

    utf8_offset
}

#[cfg(target_os = "macos")]
fn text_offset_to_utf16(text: &str, offset: usize) -> usize {
    let mut utf16_offset = 0;
    let mut utf8_count = 0;

    for ch in text.chars() {
        if utf8_count >= offset {
            break;
        }
        utf8_count += ch.len_utf8();
        utf16_offset += ch.len_utf16();
    }

    utf16_offset
}

#[cfg(target_os = "macos")]
fn clamp_text_offset(text: &str, offset: usize) -> usize {
    let mut clamped = offset.min(text.len());
    while clamped > 0 && !text.is_char_boundary(clamped) {
        clamped -= 1;
    }
    clamped
}

#[cfg(target_os = "macos")]
fn clamp_text_range(text: &str, range: &Range<usize>) -> Range<usize> {
    let start = clamp_text_offset(text, range.start);
    let end = clamp_text_offset(text, range.end);
    if end < start {
        start..start
    } else {
        start..end
    }
}

#[cfg(target_os = "macos")]
fn text_range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    let range = clamp_text_range(text, range);
    text_offset_to_utf16(text, range.start)..text_offset_to_utf16(text, range.end)
}

#[cfg(target_os = "macos")]
fn text_range_from_utf16(text: &str, range_utf16: &Range<usize>) -> Range<usize> {
    clamp_text_range(
        text,
        &(text_offset_from_utf16(text, range_utf16.start)
            ..text_offset_from_utf16(text, range_utf16.end)),
    )
}

#[cfg(target_os = "macos")]
fn previous_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn next_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .find_map(|(idx, _)| (idx > offset).then_some(idx))
        .unwrap_or(text.len())
}

#[cfg(target_os = "macos")]
impl LauncherView {
    pub(super) fn query_input_enabled(&self) -> bool {
        self.info_editor_target_id.is_none()
            && self.tag_editor_target_id.is_none()
            && self.bowl_editor_target_id.is_none()
            && self.parameter_editor_target_id.is_none()
            && self.parameter_fill_target_id.is_none()
            && !self.transform_menu_open
    }

    pub(super) fn apply_pending_text_input_focus(&mut self, window: &mut Window) {
        let Some(target) = self.pending_text_input_focus.take() else {
            return;
        };
        if self.text_input_is_visible(target) {
            window.focus(&self.text_input_focus_handle(target));
        }
    }

    pub(super) fn text_input_focus_handle(&self, target: TextInputTarget) -> FocusHandle {
        self.text_input_state(target).focus_handle.clone()
    }

    pub(super) fn text_input_state(&self, target: TextInputTarget) -> &TextInputState {
        match target {
            TextInputTarget::Query => &self.query_input_state,
            TextInputTarget::InfoEditor => &self.info_editor_input_state,
            TextInputTarget::TagEditor => &self.tag_editor_input_state,
            TextInputTarget::BowlEditor => &self.bowl_editor_input_state,
            TextInputTarget::ParameterName => &self.parameter_name_input_state,
            TextInputTarget::ParameterFill => &self.parameter_fill_input_state,
        }
    }

    pub(super) fn text_input_state_mut(&mut self, target: TextInputTarget) -> &mut TextInputState {
        match target {
            TextInputTarget::Query => &mut self.query_input_state,
            TextInputTarget::InfoEditor => &mut self.info_editor_input_state,
            TextInputTarget::TagEditor => &mut self.tag_editor_input_state,
            TextInputTarget::BowlEditor => &mut self.bowl_editor_input_state,
            TextInputTarget::ParameterName => &mut self.parameter_name_input_state,
            TextInputTarget::ParameterFill => &mut self.parameter_fill_input_state,
        }
    }

    pub(super) fn text_input_content(&self, target: TextInputTarget) -> &str {
        match target {
            TextInputTarget::Query => &self.query,
            TextInputTarget::InfoEditor => &self.info_editor_input,
            TextInputTarget::TagEditor => &self.tag_editor_input,
            TextInputTarget::BowlEditor => &self.bowl_editor_input,
            TextInputTarget::ParameterName => self
                .parameter_editor_name_inputs
                .get(self.parameter_editor_name_focus_index)
                .map(String::as_str)
                .unwrap_or(""),
            TextInputTarget::ParameterFill => self
                .parameter_fill_values
                .get(self.parameter_fill_focus_index)
                .map(String::as_str)
                .unwrap_or(""),
        }
    }

    pub(super) fn normalize_text_input_value(content: &str) -> String {
        if !content.contains('\n') && !content.contains('\r') {
            return content.to_owned();
        }

        let mut normalized = String::with_capacity(content.len());
        let mut previous_was_line_break = false;
        for ch in content.chars() {
            match ch {
                '\n' | '\r' => {
                    if !previous_was_line_break {
                        normalized.push(' ');
                        previous_was_line_break = true;
                    }
                }
                _ => {
                    normalized.push(ch);
                    previous_was_line_break = false;
                }
            }
        }

        normalized
    }

    pub(super) fn set_text_input_content(&mut self, target: TextInputTarget, content: String) {
        let content = Self::normalize_text_input_value(&content);
        match target {
            TextInputTarget::Query => self.query = content,
            TextInputTarget::InfoEditor => self.info_editor_input = content,
            TextInputTarget::TagEditor => self.tag_editor_input = content,
            TextInputTarget::BowlEditor => self.bowl_editor_input = content,
            TextInputTarget::ParameterName => {
                if self.parameter_editor_name_inputs.is_empty() {
                    self.parameter_editor_name_inputs.push(String::new());
                    self.parameter_editor_name_focus_index = 0;
                }
                let max_ix = self.parameter_editor_name_inputs.len().saturating_sub(1);
                if self.parameter_editor_name_focus_index > max_ix {
                    self.parameter_editor_name_focus_index = max_ix;
                }
                if let Some(active) = self
                    .parameter_editor_name_inputs
                    .get_mut(self.parameter_editor_name_focus_index)
                {
                    *active = content;
                }
            }
            TextInputTarget::ParameterFill => {
                if self.parameter_fill_values.is_empty() {
                    self.parameter_fill_values.push(String::new());
                    self.parameter_fill_focus_index = 0;
                }
                let max_ix = self.parameter_fill_values.len().saturating_sub(1);
                if self.parameter_fill_focus_index > max_ix {
                    self.parameter_fill_focus_index = max_ix;
                }
                if let Some(active) = self
                    .parameter_fill_values
                    .get_mut(self.parameter_fill_focus_index)
                {
                    *active = content;
                }
            }
        }
    }

    fn text_input_is_visible(&self, target: TextInputTarget) -> bool {
        match target {
            TextInputTarget::Query => self.query_input_enabled(),
            TextInputTarget::InfoEditor => self.info_editor_target_id.is_some(),
            TextInputTarget::TagEditor => self.tag_editor_target_id.is_some(),
            TextInputTarget::BowlEditor => self.bowl_editor_target_id.is_some(),
            TextInputTarget::ParameterName => {
                self.parameter_editor_target_id.is_some()
                    && self.parameter_editor_stage == ParameterEditorStage::EnterName
                    && !self.parameter_editor_selected_targets.is_empty()
            }
            TextInputTarget::ParameterFill => {
                self.parameter_fill_target_id.is_some() && !self.parameter_fill_values.is_empty()
            }
        }
    }

    fn active_text_input_target(&self, window: &Window) -> Option<TextInputTarget> {
        [
            TextInputTarget::InfoEditor,
            TextInputTarget::TagEditor,
            TextInputTarget::BowlEditor,
            TextInputTarget::ParameterName,
            TextInputTarget::ParameterFill,
            TextInputTarget::Query,
        ]
        .into_iter()
        .find(|target| {
            self.text_input_is_visible(*target)
                && self.text_input_focus_handle(*target).is_focused(window)
        })
    }

    fn text_input_cursor_offset(&self, target: TextInputTarget) -> usize {
        self.text_input_state(target).cursor_offset()
    }

    fn set_cursor_to(&mut self, target: TextInputTarget, offset: usize) {
        let input_state = self.text_input_state_mut(target);
        input_state.selected_range = offset..offset;
        input_state.selection_reversed = false;
        input_state.marked_range = None;
    }

    fn text_input_move_to(
        &mut self,
        target: TextInputTarget,
        offset: usize,
        cx: &mut Context<Self>,
    ) {
        self.set_cursor_to(target, offset);
        cx.notify();
    }

    fn set_select_to(&mut self, target: TextInputTarget, offset: usize) {
        let input_state = self.text_input_state_mut(target);
        if input_state.selection_reversed {
            input_state.selected_range.start = offset;
        } else {
            input_state.selected_range.end = offset;
        }
        if input_state.selected_range.end < input_state.selected_range.start {
            input_state.selection_reversed = !input_state.selection_reversed;
            input_state.selected_range =
                input_state.selected_range.end..input_state.selected_range.start;
        }
    }

    fn text_input_select_to(
        &mut self,
        target: TextInputTarget,
        offset: usize,
        cx: &mut Context<Self>,
    ) {
        self.set_select_to(target, offset);
        cx.notify();
    }

    fn text_input_index_for_mouse_position(
        &self,
        target: TextInputTarget,
        position: Point<Pixels>,
    ) -> usize {
        let content = self.text_input_content(target);
        if content.is_empty() {
            return 0;
        }

        let input_state = self.text_input_state(target);
        let (Some(bounds), Some(line)) = (
            input_state.last_bounds.as_ref(),
            input_state.last_layout.as_ref(),
        ) else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return content.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn text_input_previous_boundary(&self, target: TextInputTarget, offset: usize) -> usize {
        previous_boundary(self.text_input_content(target), offset)
    }

    fn text_input_next_boundary(&self, target: TextInputTarget, offset: usize) -> usize {
        next_boundary(self.text_input_content(target), offset)
    }

    fn text_input_change_did_commit(&mut self, target: TextInputTarget, cx: &mut Context<Self>) {
        if target == TextInputTarget::Query {
            self.query_did_change(cx);
        } else if target == TextInputTarget::BowlEditor {
            self.bowl_editor_suggestions =
                self.storage.suggest_bowl_names(&self.bowl_editor_input, 6);
            cx.notify();
        } else {
            cx.notify();
        }
    }

    fn replace_text_for_target(
        &mut self,
        target: TextInputTarget,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        mark_text: bool,
        cx: &mut Context<Self>,
    ) {
        let content = self.text_input_content(target).to_owned();
        let input_state = self.text_input_state(target);
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| text_range_from_utf16(&content, range_utf16))
            .or_else(|| {
                input_state
                    .marked_range
                    .as_ref()
                    .map(|range| clamp_text_range(&content, range))
            })
            .unwrap_or_else(|| clamp_text_range(&content, &input_state.selected_range));

        let normalized_new_text = Self::normalize_text_input_value(new_text);
        let updated_content =
            content[0..range.start].to_owned() + &normalized_new_text + &content[range.end..];
        let selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| text_range_from_utf16(&normalized_new_text, range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.start)
            .unwrap_or_else(|| {
                range.start + normalized_new_text.len()..range.start + normalized_new_text.len()
            });
        let marked_range = (mark_text && !normalized_new_text.is_empty())
            .then_some(range.start..range.start + normalized_new_text.len());

        self.set_text_input_content(target, updated_content);
        let input_state = self.text_input_state_mut(target);
        input_state.selected_range = selected_range;
        input_state.selection_reversed = false;
        input_state.marked_range = marked_range;
        self.text_input_change_did_commit(target, cx);
    }

    pub(super) fn query_backspace(
        &mut self,
        _: &QueryBackspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.active_text_input_target(window) else {
            return;
        };
        if self.text_input_state(target).selected_range.is_empty() {
            let previous =
                self.text_input_previous_boundary(target, self.text_input_cursor_offset(target));
            self.set_select_to(target, previous);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    pub(super) fn query_left(
        &mut self,
        _: &QueryLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.active_text_input_target(window) else {
            return;
        };
        if self.text_input_state(target).selected_range.is_empty() {
            self.text_input_move_to(
                target,
                self.text_input_previous_boundary(target, self.text_input_cursor_offset(target)),
                cx,
            );
        } else {
            self.text_input_move_to(
                target,
                self.text_input_state(target).selected_range.start,
                cx,
            );
        }
    }

    pub(super) fn query_right(
        &mut self,
        _: &QueryRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.active_text_input_target(window) else {
            return;
        };
        if self.text_input_state(target).selected_range.is_empty() {
            self.text_input_move_to(
                target,
                self.text_input_next_boundary(target, self.text_input_cursor_offset(target)),
                cx,
            );
        } else {
            self.text_input_move_to(target, self.text_input_state(target).selected_range.end, cx);
        }
    }

    pub(super) fn query_select_left(
        &mut self,
        _: &QuerySelectLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.active_text_input_target(window) else {
            return;
        };
        let previous =
            self.text_input_previous_boundary(target, self.text_input_cursor_offset(target));
        self.text_input_select_to(target, previous, cx);
    }

    pub(super) fn query_select_right(
        &mut self,
        _: &QuerySelectRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.active_text_input_target(window) else {
            return;
        };
        let next = self.text_input_next_boundary(target, self.text_input_cursor_offset(target));
        self.text_input_select_to(target, next, cx);
    }

    pub(super) fn query_select_all(
        &mut self,
        _: &QuerySelectAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.active_text_input_target(window) else {
            return;
        };
        self.set_cursor_to(target, 0);
        self.set_select_to(target, self.text_input_content(target).len());
        cx.notify();
    }

    pub(super) fn query_home(
        &mut self,
        _: &QueryHome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.active_text_input_target(window) {
            self.text_input_move_to(target, 0, cx);
        }
    }

    pub(super) fn query_end(&mut self, _: &QueryEnd, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.active_text_input_target(window) {
            self.text_input_move_to(target, self.text_input_content(target).len(), cx);
        }
    }

    pub(super) fn query_show_character_palette(
        &mut self,
        _: &QueryShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        if self.active_text_input_target(window).is_some() {
            window.show_character_palette();
        }
    }

    pub(super) fn query_paste(
        &mut self,
        _: &QueryPaste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_text_input_target(window).is_none() {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    pub(super) fn query_copy(
        &mut self,
        _: &QueryCopy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.active_text_input_target(window) else {
            return;
        };
        let content = self.text_input_content(target).to_owned();
        let selection = clamp_text_range(&content, &self.text_input_state(target).selected_range);
        if !selection.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(content[selection].to_string()));
        }
    }

    pub(super) fn query_cut(&mut self, _: &QueryCut, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.active_text_input_target(window) else {
            return;
        };
        let content = self.text_input_content(target).to_owned();
        let selection = clamp_text_range(&content, &self.text_input_state(target).selected_range);
        if !selection.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(content[selection].to_string()));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    pub(super) fn text_input_on_mouse_down(
        &mut self,
        target: TextInputTarget,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.text_input_focus_handle(target));
        self.pending_text_input_focus = None;
        self.text_input_state_mut(target).is_selecting = true;

        if event.modifiers.shift {
            let index = self.text_input_index_for_mouse_position(target, event.position);
            self.text_input_select_to(target, index, cx);
        } else {
            let index = self.text_input_index_for_mouse_position(target, event.position);
            self.text_input_move_to(target, index, cx);
        }
    }

    pub(super) fn text_input_on_mouse_up(
        &mut self,
        target: TextInputTarget,
        _: &MouseUpEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.text_input_state_mut(target).is_selecting = false;
    }

    pub(super) fn text_input_on_mouse_move(
        &mut self,
        target: TextInputTarget,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.text_input_state(target).is_selecting {
            let index = self.text_input_index_for_mouse_position(target, event.position);
            self.text_input_select_to(target, index, cx);
        }
    }
}

#[cfg(target_os = "macos")]
impl Focusable for LauncherView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.query_input_state.focus_handle.clone()
    }
}

#[cfg(target_os = "macos")]
impl EntityInputHandler for LauncherView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let target = self.active_text_input_target(window)?;
        let content = self.text_input_content(target).to_owned();
        let range = text_range_from_utf16(&content, &range_utf16);
        actual_range.replace(text_range_to_utf16(&content, &range));
        Some(content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let target = self.active_text_input_target(window)?;
        let content = self.text_input_content(target).to_owned();
        let input_state = self.text_input_state(target);
        let selected_range = clamp_text_range(&content, &input_state.selected_range);
        Some(UTF16Selection {
            range: text_range_to_utf16(&content, &selected_range),
            reversed: input_state.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let target = self.active_text_input_target(window)?;
        let content = self.text_input_content(target).to_owned();
        self.text_input_state(target)
            .marked_range
            .as_ref()
            .map(|range| text_range_to_utf16(&content, &clamp_text_range(&content, range)))
    }

    fn unmark_text(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(target) = self.active_text_input_target(window) {
            self.text_input_state_mut(target).marked_range = None;
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.active_text_input_target(window) {
            self.replace_text_for_target(target, range_utf16, new_text, None, false, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.active_text_input_target(window) {
            self.replace_text_for_target(
                target,
                range_utf16,
                new_text,
                new_selected_range_utf16,
                true,
                cx,
            );
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let target = self.active_text_input_target(window)?;
        let content = self.text_input_content(target).to_owned();
        let last_layout = self.text_input_state(target).last_layout.as_ref()?;
        let range = text_range_from_utf16(&content, &range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let target = self.active_text_input_target(window)?;
        let content = self.text_input_content(target).to_owned();
        if content.is_empty() {
            return Some(0);
        }

        let input_state = self.text_input_state(target);
        let bounds = input_state.last_bounds?;
        let last_layout = input_state.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(point.x - bounds.left())?;
        Some(text_offset_to_utf16(&content, utf8_index))
    }
}

#[cfg(all(target_os = "macos", test))]
mod tests {
    use super::*;

    #[test]
    fn clamp_text_range_handles_empty_content() {
        assert_eq!(clamp_text_range("", &(5..5)), 0..0);
        assert_eq!(clamp_text_range("", &(2..9)), 0..0);
    }

    #[test]
    fn text_range_from_utf16_clamps_to_current_content() {
        assert_eq!(text_range_from_utf16("", &(4..4)), 0..0);
        assert_eq!(text_range_from_utf16("abc", &(5..5)), 3..3);
    }

    #[test]
    fn normalize_text_input_value_replaces_line_break_runs() {
        assert_eq!(
            LauncherView::normalize_text_input_value("alpha\r\nbeta\ngamma"),
            "alpha beta gamma"
        );
        assert_eq!(
            LauncherView::normalize_text_input_value("first\n\nsecond"),
            "first second"
        );
    }
}
