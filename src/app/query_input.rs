#[cfg(target_os = "macos")]
use crate::*;
#[cfg(target_os = "macos")]
use unicode_segmentation::UnicodeSegmentation;

#[cfg(target_os = "macos")]
const QUERY_PLACEHOLDER: &str = "Search snippets, commands, and passwords…";

#[cfg(target_os = "macos")]
pub(super) struct QueryInputElement {
    pub(super) input: Entity<LauncherView>,
    pub(super) palette: Palette,
    pub(super) enabled: bool,
}

#[cfg(target_os = "macos")]
pub(super) struct QueryPrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

#[cfg(target_os = "macos")]
impl QueryInputElement {
    pub(super) fn new(input: Entity<LauncherView>, palette: Palette, enabled: bool) -> Self {
        Self {
            input,
            palette,
            enabled,
        }
    }
}

#[cfg(target_os = "macos")]
impl IntoElement for QueryInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(target_os = "macos")]
impl GpuiElement for QueryInputElement {
    type RequestLayoutState = ();
    type PrepaintState = QueryPrepaintState;

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
        let content = input.query.clone();
        let selected_range = input.query_selected_range.clone();
        let cursor = input.query_cursor_offset();
        let style = window.text_style();

        let (display_text, text_color): (SharedString, gpui::Hsla) = if content.is_empty() {
            (QUERY_PLACEHOLDER.into(), self.palette.query_placeholder.into())
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
            if let Some(marked_range) = input.query_marked_range.as_ref() {
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

        QueryPrepaintState {
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
        let focus_handle = self.input.read(cx).query_focus_handle.clone();
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
        let line = prepaint.line.take().unwrap();
        line.paint(bounds.origin, window.line_height(), window, cx)
            .unwrap();

        if self.enabled
            && focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.query_last_layout = Some(line);
            input.query_last_bounds = Some(bounds);
        });
    }
}

#[cfg(target_os = "macos")]
impl LauncherView {
    pub(super) fn query_input_enabled(&self) -> bool {
        self.info_editor_target_id.is_none()
            && self.tag_editor_target_id.is_none()
            && self.parameter_editor_target_id.is_none()
            && self.parameter_fill_target_id.is_none()
            && !self.transform_menu_open
    }

    pub(super) fn query_cursor_offset(&self) -> usize {
        if self.query_selection_reversed {
            self.query_selected_range.start
        } else {
            self.query_selected_range.end
        }
    }

    pub(super) fn query_move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.query_selected_range = offset..offset;
        self.query_selection_reversed = false;
        self.query_marked_range = None;
        cx.notify();
    }

    pub(super) fn query_select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.query_selection_reversed {
            self.query_selected_range.start = offset;
        } else {
            self.query_selected_range.end = offset;
        }
        if self.query_selected_range.end < self.query_selected_range.start {
            self.query_selection_reversed = !self.query_selection_reversed;
            self.query_selected_range =
                self.query_selected_range.end..self.query_selected_range.start;
        }
        cx.notify();
    }

    pub(super) fn query_index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.query.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (
            self.query_last_bounds.as_ref(),
            self.query_last_layout.as_ref(),
        ) else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.query.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn query_offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.query.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn query_offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.query.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn query_range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.query_offset_to_utf16(range.start)..self.query_offset_to_utf16(range.end)
    }

    fn query_range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.query_offset_from_utf16(range_utf16.start)..self.query_offset_from_utf16(range_utf16.end)
    }

    fn query_previous_boundary(&self, offset: usize) -> usize {
        self.query
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn query_next_boundary(&self, offset: usize) -> usize {
        self.query
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.query.len())
    }

    pub(super) fn query_backspace(
        &mut self,
        _: &QueryBackspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.query_selected_range.is_empty() {
            self.query_select_to(self.query_previous_boundary(self.query_cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    pub(super) fn query_left(&mut self, _: &QueryLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.query_selected_range.is_empty() {
            self.query_move_to(self.query_previous_boundary(self.query_cursor_offset()), cx);
        } else {
            self.query_move_to(self.query_selected_range.start, cx);
        }
    }

    pub(super) fn query_right(&mut self, _: &QueryRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.query_selected_range.is_empty() {
            self.query_move_to(self.query_next_boundary(self.query_cursor_offset()), cx);
        } else {
            self.query_move_to(self.query_selected_range.end, cx);
        }
    }

    pub(super) fn query_select_left(
        &mut self,
        _: &QuerySelectLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.query_select_to(self.query_previous_boundary(self.query_cursor_offset()), cx);
    }

    pub(super) fn query_select_right(
        &mut self,
        _: &QuerySelectRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.query_select_to(self.query_next_boundary(self.query_cursor_offset()), cx);
    }

    pub(super) fn query_select_all(
        &mut self,
        _: &QuerySelectAll,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.query_move_to(0, cx);
        self.query_select_to(self.query.len(), cx);
    }

    pub(super) fn query_home(
        &mut self,
        _: &QueryHome,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.query_move_to(0, cx);
    }

    pub(super) fn query_end(
        &mut self,
        _: &QueryEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.query_move_to(self.query.len(), cx);
    }

    pub(super) fn query_show_character_palette(
        &mut self,
        _: &QueryShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    pub(super) fn query_paste(
        &mut self,
        _: &QueryPaste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
        }
    }

    pub(super) fn query_copy(
        &mut self,
        _: &QueryCopy,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.query_selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.query[self.query_selected_range.clone()].to_string(),
            ));
        }
    }

    pub(super) fn query_cut(
        &mut self,
        _: &QueryCut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.query_selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.query[self.query_selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    pub(super) fn query_on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.query_focus_handle);
        self.query_is_selecting = true;

        if event.modifiers.shift {
            self.query_select_to(self.query_index_for_mouse_position(event.position), cx);
        } else {
            self.query_move_to(self.query_index_for_mouse_position(event.position), cx);
        }
    }

    pub(super) fn query_on_mouse_up(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.query_is_selecting = false;
    }

    pub(super) fn query_on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.query_is_selecting {
            self.query_select_to(self.query_index_for_mouse_position(event.position), cx);
        }
    }
}

#[cfg(target_os = "macos")]
impl Focusable for LauncherView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.query_focus_handle.clone()
    }
}

#[cfg(target_os = "macos")]
impl EntityInputHandler for LauncherView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.query_range_from_utf16(&range_utf16);
        actual_range.replace(self.query_range_to_utf16(&range));
        Some(self.query[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.query_range_to_utf16(&self.query_selected_range),
            reversed: self.query_selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.query_marked_range
            .as_ref()
            .map(|range| self.query_range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.query_marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.query_range_from_utf16(range_utf16))
            .or(self.query_marked_range.clone())
            .unwrap_or(self.query_selected_range.clone());

        self.query = self.query[0..range.start].to_owned() + new_text + &self.query[range.end..];
        self.query_selected_range =
            range.start + new_text.len()..range.start + new_text.len();
        self.query_selection_reversed = false;
        self.query_marked_range.take();
        self.query_did_change(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.query_range_from_utf16(range_utf16))
            .or(self.query_marked_range.clone())
            .unwrap_or(self.query_selected_range.clone());

        self.query = self.query[0..range.start].to_owned() + new_text + &self.query[range.end..];
        self.query_marked_range = (!new_text.is_empty()).then_some(range.start..range.start + new_text.len());
        self.query_selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.query_range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.start)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.query_selection_reversed = false;

        self.query_did_change(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.query_last_layout.as_ref()?;
        let range = self.query_range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(bounds.left() + last_layout.x_for_index(range.start), bounds.top()),
            point(bounds.left() + last_layout.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        if self.query.is_empty() {
            return Some(0);
        }

        let line_point = self.query_last_bounds?.localize(&point)?;
        let last_layout = self.query_last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(point.x - line_point.x)?;
        Some(self.query_offset_to_utf16(utf8_index))
    }
}
