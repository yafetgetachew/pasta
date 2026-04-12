use super::actions::{
    expand_candidates_with_splits, has_structured_parameter_candidates,
    parameter_clickable_candidates,
};
use super::query_input::TextInputElement;
use super::state::CachedRowPresentation;
use crate::*;
use gpui::{AnyElement, StatefulInteractiveElement};

impl Render for LauncherView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_pending_text_input_focus(window);
        let palette = palette_for(window.appearance(), self.surface_alpha);
        let info_editor_open = self.info_editor_target_id.is_some();
        let tag_editor_open = self.tag_editor_target_id.is_some();
        let bowl_editor_open = self.bowl_editor_target_id.is_some();
        let parameter_editor_open = self.parameter_editor_target_id.is_some();
        let parameter_fill_open = self.parameter_fill_target_id.is_some();
        let transform_menu_open = self.transform_menu_open;
        let query_input_enabled = self.query_input_enabled();
        let query_focus_handle = self.text_input_focus_handle(TextInputTarget::Query);
        let query_focused = query_focus_handle.is_focused(window);

        let results = if self.items.is_empty() {
            div()
                .id("results-list")
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(palette.muted_text)
                .text_sm()
                .child("Nothing copied yet.")
                .into_any_element()
        } else {
            uniform_list(
                "results-list",
                self.items.len(),
                cx.processor(move |this, range: Range<usize>, _window, cx| {
                    let mut rows = Vec::with_capacity(range.end.saturating_sub(range.start));
                    for ix in range {
                        if let (Some(item), Some(row_data)) =
                            (this.items.get(ix), this.row_presentations.get(ix))
                        {
                            rows.push(this.render_result_row(
                                ix,
                                item,
                                row_data,
                                palette,
                                info_editor_open,
                                tag_editor_open,
                                bowl_editor_open,
                                parameter_editor_open,
                                parameter_fill_open,
                                transform_menu_open,
                                cx,
                            ));
                        }
                    }
                    rows
                }),
            )
            .w_full()
            .h_full()
            .track_scroll(self.results_scroll.clone())
            .into_any_element()
        };

        let mut panel = div()
            .size_full()
            .font_family(self.font_family.clone())
            .font_weight(FontWeight::LIGHT)
            .opacity(self.transition_alpha)
            .bg(palette.window_bg)
            .border_1()
            .border_color(palette.window_border)
            .rounded_2xl()
            .overflow_hidden();
        if self.transition_target > 0.0 && self.transition_alpha > 0.35 {
            panel = panel.shadow_xl();
        }

        let mut content = panel
            .px_4()
            .py_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.title_text)
                            .child("PASTA"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child(if cfg!(target_os = "macos") {
                                "⌥ + SPACE"
                            } else {
                                "Meta + Space"
                            }),
                    ),
            )
            .child({
                let mut query_container = div()
                    .w_full()
                    .px_2()
                    .py(px(2.0))
                    .rounded_md()
                    .line_height(px(30.0))
                    .text_lg()
                    .font_weight(FontWeight::NORMAL);

                if query_input_enabled {
                    query_container = query_container
                        .key_context("PastaTextInput")
                        .track_focus(&query_focus_handle)
                        .cursor(CursorStyle::IBeam)
                        .on_action(cx.listener(Self::query_backspace))
                        .on_action(cx.listener(Self::query_left))
                        .on_action(cx.listener(Self::query_right))
                        .on_action(cx.listener(Self::query_select_left))
                        .on_action(cx.listener(Self::query_select_right))
                        .on_action(cx.listener(Self::query_select_all))
                        .on_action(cx.listener(Self::query_home))
                        .on_action(cx.listener(Self::query_end))
                        .on_action(cx.listener(Self::query_show_character_palette))
                        .on_action(cx.listener(Self::query_paste))
                        .on_action(cx.listener(Self::query_cut))
                        .on_action(cx.listener(Self::query_copy))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event, window, cx| {
                                this.text_input_on_mouse_down(
                                    TextInputTarget::Query,
                                    event,
                                    window,
                                    cx,
                                );
                            }),
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, event, window, cx| {
                                this.text_input_on_mouse_up(
                                    TextInputTarget::Query,
                                    event,
                                    window,
                                    cx,
                                );
                            }),
                        )
                        .on_mouse_up_out(
                            MouseButton::Left,
                            cx.listener(|this, event, window, cx| {
                                this.text_input_on_mouse_up(
                                    TextInputTarget::Query,
                                    event,
                                    window,
                                    cx,
                                );
                            }),
                        )
                        .on_mouse_move(cx.listener(|this, event, window, cx| {
                            this.text_input_on_mouse_move(
                                TextInputTarget::Query,
                                event,
                                window,
                                cx,
                            );
                        }));
                }

                if query_focused && query_input_enabled {
                    query_container = query_container
                        .bg(scale_alpha(
                            palette.selected_bg,
                            if palette.dark { 0.95 } else { 0.75 },
                        ))
                        .border_1()
                        .border_color(scale_alpha(
                            palette.selected_border,
                            if palette.dark { 0.58 } else { 0.52 },
                        ));
                }

                div()
                    .w_full()
                    .child(query_container.child(TextInputElement::new(
                        cx.entity(),
                        TextInputTarget::Query,
                        "Search snippets, commands, and passwords…",
                        palette,
                        query_input_enabled,
                    )))
            });
        if !self.tag_search_suggestions.is_empty() && query_input_enabled {
            content = content.child(self.render_tag_search_suggestions(palette, cx));
        }

        if let Some(item_id) = self.info_editor_target_id {
            let info_editor_focus_handle =
                self.text_input_focus_handle(TextInputTarget::InfoEditor);
            let info_editor_focused = info_editor_focus_handle.is_focused(window);
            let mut info_input = div()
                .w_full()
                .mt_1()
                .px_1()
                .rounded_md()
                .line_height(px(24.0))
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .key_context("PastaTextInput")
                .track_focus(&info_editor_focus_handle)
                .cursor(CursorStyle::IBeam)
                .on_action(cx.listener(Self::query_backspace))
                .on_action(cx.listener(Self::query_left))
                .on_action(cx.listener(Self::query_right))
                .on_action(cx.listener(Self::query_select_left))
                .on_action(cx.listener(Self::query_select_right))
                .on_action(cx.listener(Self::query_select_all))
                .on_action(cx.listener(Self::query_home))
                .on_action(cx.listener(Self::query_end))
                .on_action(cx.listener(Self::query_show_character_palette))
                .on_action(cx.listener(Self::query_paste))
                .on_action(cx.listener(Self::query_cut))
                .on_action(cx.listener(Self::query_copy))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event, window, cx| {
                        this.text_input_on_mouse_down(
                            TextInputTarget::InfoEditor,
                            event,
                            window,
                            cx,
                        );
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, event, window, cx| {
                        this.text_input_on_mouse_up(TextInputTarget::InfoEditor, event, window, cx);
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, event, window, cx| {
                        this.text_input_on_mouse_up(TextInputTarget::InfoEditor, event, window, cx);
                    }),
                )
                .on_mouse_move(cx.listener(|this, event, window, cx| {
                    this.text_input_on_mouse_move(TextInputTarget::InfoEditor, event, window, cx);
                }));

            if info_editor_focused {
                info_input = info_input
                    .bg(scale_alpha(
                        palette.selected_bg,
                        if palette.dark { 0.95 } else { 0.75 },
                    ))
                    .border_1()
                    .border_color(palette.selected_border);
            }

            content = content.child(
                div()
                    .w_full()
                    .p_2()
                    .bg(scale_alpha(
                        palette.row_hover_bg,
                        if palette.dark { 0.95 } else { 1.0 },
                    ))
                    .border_1()
                    .border_color(palette.selected_border)
                    .rounded_lg()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(palette.title_text)
                                    .child(format!("Snippet Info • Snippet #{item_id}")),
                            ),
                    )
                    .child(info_input.child(TextInputElement::new(
                        cx.entity(),
                        TextInputTarget::InfoEditor,
                        "Add info…",
                        palette,
                        true,
                    )))
                    .child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child(if cfg!(target_os = "macos") {
                                "⌘V paste"
                            } else {
                                "Ctrl+V paste"
                            }),
                    ),
            );
        }

        if let Some(item_id) = self.tag_editor_target_id {
            let tag_editor_focus_handle = self.text_input_focus_handle(TextInputTarget::TagEditor);
            let tag_editor_focused = tag_editor_focus_handle.is_focused(window);
            let mut tag_input = div()
                .w_full()
                .mt_1()
                .px_1()
                .rounded_md()
                .line_height(px(24.0))
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .key_context("PastaTextInput")
                .track_focus(&tag_editor_focus_handle)
                .cursor(CursorStyle::IBeam)
                .on_action(cx.listener(Self::query_backspace))
                .on_action(cx.listener(Self::query_left))
                .on_action(cx.listener(Self::query_right))
                .on_action(cx.listener(Self::query_select_left))
                .on_action(cx.listener(Self::query_select_right))
                .on_action(cx.listener(Self::query_select_all))
                .on_action(cx.listener(Self::query_home))
                .on_action(cx.listener(Self::query_end))
                .on_action(cx.listener(Self::query_show_character_palette))
                .on_action(cx.listener(Self::query_paste))
                .on_action(cx.listener(Self::query_cut))
                .on_action(cx.listener(Self::query_copy))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event, window, cx| {
                        this.text_input_on_mouse_down(
                            TextInputTarget::TagEditor,
                            event,
                            window,
                            cx,
                        );
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, event, window, cx| {
                        this.text_input_on_mouse_up(TextInputTarget::TagEditor, event, window, cx);
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, event, window, cx| {
                        this.text_input_on_mouse_up(TextInputTarget::TagEditor, event, window, cx);
                    }),
                )
                .on_mouse_move(cx.listener(|this, event, window, cx| {
                    this.text_input_on_mouse_move(TextInputTarget::TagEditor, event, window, cx);
                }));
            if tag_editor_focused {
                tag_input = tag_input
                    .bg(scale_alpha(
                        palette.selected_bg,
                        if palette.dark { 0.95 } else { 0.75 },
                    ))
                    .border_1()
                    .border_color(palette.selected_border);
            }
            let title = if self.tag_editor_mode == TagEditorMode::Add {
                "Add Custom Tags"
            } else {
                "Remove Tags"
            };

            content = content.child(
                div()
                    .w_full()
                    .p_2()
                    .bg(scale_alpha(
                        palette.row_hover_bg,
                        if palette.dark { 0.95 } else { 1.0 },
                    ))
                    .border_1()
                    .border_color(palette.selected_border)
                    .rounded_lg()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(palette.title_text)
                                    .child(format!("{title} • Snippet #{item_id}")),
                            ),
                    )
                    .child(tag_input.child(TextInputElement::new(
                        cx.entity(),
                        TextInputTarget::TagEditor,
                        "tag1,tag2",
                        palette,
                        true,
                    )))
                    .child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child(if cfg!(target_os = "macos") {
                                "comma-separated • ⌘V"
                            } else {
                                "comma-separated • Ctrl+V"
                            }),
                    ),
            );
        }

        if let Some(item_id) = self.bowl_editor_target_id {
            let bowl_editor_focus_handle =
                self.text_input_focus_handle(TextInputTarget::BowlEditor);
            let bowl_editor_focused = bowl_editor_focus_handle.is_focused(window);
            let mut bowl_input = div()
                .w_full()
                .mt_1()
                .px_1()
                .rounded_md()
                .line_height(px(24.0))
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .key_context("PastaTextInput")
                .track_focus(&bowl_editor_focus_handle)
                .cursor(CursorStyle::IBeam)
                .on_action(cx.listener(Self::query_backspace))
                .on_action(cx.listener(Self::query_left))
                .on_action(cx.listener(Self::query_right))
                .on_action(cx.listener(Self::query_select_left))
                .on_action(cx.listener(Self::query_select_right))
                .on_action(cx.listener(Self::query_select_all))
                .on_action(cx.listener(Self::query_home))
                .on_action(cx.listener(Self::query_end))
                .on_action(cx.listener(Self::query_show_character_palette))
                .on_action(cx.listener(Self::query_paste))
                .on_action(cx.listener(Self::query_cut))
                .on_action(cx.listener(Self::query_copy))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event, window, cx| {
                        this.text_input_on_mouse_down(
                            TextInputTarget::BowlEditor,
                            event,
                            window,
                            cx,
                        );
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, event, window, cx| {
                        this.text_input_on_mouse_up(TextInputTarget::BowlEditor, event, window, cx);
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, event, window, cx| {
                        this.text_input_on_mouse_up(TextInputTarget::BowlEditor, event, window, cx);
                    }),
                )
                .on_mouse_move(cx.listener(|this, event, window, cx| {
                    this.text_input_on_mouse_move(TextInputTarget::BowlEditor, event, window, cx);
                }));
            if bowl_editor_focused {
                bowl_input = bowl_input
                    .bg(scale_alpha(
                        palette.selected_bg,
                        if palette.dark { 0.95 } else { 0.75 },
                    ))
                    .border_1()
                    .border_color(palette.selected_border);
            }

            let mut bowl_panel = div()
                .w_full()
                .p_2()
                .bg(scale_alpha(
                    palette.row_hover_bg,
                    if palette.dark { 0.95 } else { 1.0 },
                ))
                .border_1()
                .border_color(palette.selected_border)
                .rounded_lg()
                .child(
                    div()
                        .w_full()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .text_sm()
                                .text_color(palette.title_text)
                                .child(format!("Assign Bowl • Snippet #{item_id}")),
                        ),
                )
                .child(bowl_input.child(TextInputElement::new(
                    cx.entity(),
                    TextInputTarget::BowlEditor,
                    "BOWL-NAME",
                    palette,
                    true,
                )));

            if !self.bowl_editor_suggestions.is_empty() {
                let mut chips = div().w_full().mt_1().flex().flex_row().flex_wrap().gap_1();
                for (ix, suggestion) in self.bowl_editor_suggestions.iter().enumerate() {
                    let is_primary = ix == 0;
                    let chip_bg = if is_primary {
                        scale_alpha(palette.selected_bg, if palette.dark { 0.92 } else { 0.72 })
                    } else {
                        scale_alpha(palette.row_hover_bg, if palette.dark { 0.9 } else { 1.0 })
                    };
                    let chip_border = if is_primary {
                        palette.selected_border
                    } else {
                        scale_alpha(palette.window_border, if palette.dark { 0.84 } else { 0.9 })
                    };
                    let chip_text = if is_primary {
                        palette.title_text
                    } else {
                        palette.muted_text
                    };
                    let suggestion_owned = suggestion.clone();
                    chips = chips.child(
                        div()
                            .id(("bowl-editor-suggestion", ix))
                            .text_xs()
                            .text_color(chip_text)
                            .bg(chip_bg)
                            .border_1()
                            .border_color(chip_border)
                            .rounded_md()
                            .px_1()
                            .py(px(1.0))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.bowl_editor_input = suggestion_owned.clone();
                                let len = this.bowl_editor_input.len();
                                this.bowl_editor_input_state.selected_range = len..len;
                                this.bowl_editor_input_state.selection_reversed = false;
                                this.bowl_editor_input_state.marked_range = None;
                                this.bowl_editor_suggestions =
                                    this.storage.suggest_bowl_names(&this.bowl_editor_input, 6);
                                cx.notify();
                            }))
                            .child(suggestion.clone()),
                    );
                }
                bowl_panel = bowl_panel.child(chips);
            }

            let help_text = if self.bowl_editor_suggestions.is_empty() {
                if cfg!(target_os = "macos") {
                    "single bowl • blank = remove • ⌘V"
                } else {
                    "single bowl • blank = remove • Ctrl+V"
                }
            } else {
                "single bowl • Tab autocomplete • blank = remove"
            };
            bowl_panel = bowl_panel.child(
                div()
                    .w_full()
                    .mt_1()
                    .text_xs()
                    .text_color(palette.muted_text)
                    .child(help_text),
            );
            content = content.child(bowl_panel);
        }

        if let Some(item_id) = self.parameter_editor_target_id {
            if self.parameter_editor_stage == ParameterEditorStage::SelectValue {
                let item_content = self
                    .items
                    .iter()
                    .find(|entry| entry.id == item_id)
                    .map(|entry| entry.content.clone())
                    .unwrap_or_default();
                let has_structured_candidates = has_structured_parameter_candidates(&item_content);
                let candidates =
                    parameter_clickable_candidates(&item_content, self.parameter_editor_force_full);
                let candidates =
                    expand_candidates_with_splits(candidates, &self.parameter_editor_split_tokens);
                let auto_named_candidates =
                    has_structured_candidates && !self.parameter_editor_force_full;
                let mut token_picker = div().w_full().mt_1().flex().flex_row().flex_wrap().gap_1();
                for (range_ix, candidate) in candidates.into_iter().take(120).enumerate() {
                    if candidate.target.is_empty() {
                        continue;
                    }
                    let token = candidate.label;
                    let target = candidate.target;
                    let is_selected = self
                        .parameter_editor_selected_targets
                        .iter()
                        .any(|existing| existing == &target);
                    let chip_bg = if is_selected {
                        if palette.dark {
                            rgb(0x22d3ee)
                        } else {
                            rgb(0x0891b2)
                        }
                    } else {
                        scale_alpha(palette.row_hover_bg, if palette.dark { 0.92 } else { 1.0 })
                    };
                    let chip_border = if is_selected {
                        if palette.dark {
                            rgb(0x67e8f9)
                        } else {
                            rgb(0x0e7490)
                        }
                    } else {
                        scale_alpha(palette.window_border, if palette.dark { 0.85 } else { 1.0 })
                    };

                    token_picker = token_picker.child(
                        div()
                            .id(("parameter-token", range_ix as u64))
                            .text_xs()
                            .text_color(if is_selected {
                                if palette.dark {
                                    rgb(0x042f2e)
                                } else {
                                    rgb(0xffffff)
                                }
                            } else {
                                palette.row_text
                            })
                            .bg(chip_bg)
                            .border_1()
                            .border_color(chip_border)
                            .rounded_md()
                            .px_1()
                            .py(px(1.0))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                                let mods = event.modifiers();
                                let additive = if cfg!(target_os = "macos") {
                                    mods.platform
                                } else {
                                    mods.control
                                };
                                this.select_parameter_clickable_range(range_ix, additive, cx);
                            }))
                            .child(token),
                    );
                }

                let mut selector_header = div()
                    .w_full()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_sm().text_color(palette.title_text).child(
                        if auto_named_candidates {
                            format!("Select Parameters • Snippet #{item_id}")
                        } else if has_structured_candidates && self.parameter_editor_force_full {
                            format!("Full Parametrize • Snippet #{item_id}")
                        } else {
                            format!("Parametrize Snippet • Snippet #{item_id}")
                        },
                    ));

                let guided_active = has_structured_candidates && !self.parameter_editor_force_full;
                let full_active = self.parameter_editor_force_full || !has_structured_candidates;

                let guided_bg = if guided_active {
                    if palette.dark {
                        rgb(0x22d3ee)
                    } else {
                        rgb(0x0891b2)
                    }
                } else {
                    scale_alpha(palette.row_hover_bg, if palette.dark { 0.95 } else { 1.0 })
                };
                let guided_border = if guided_active {
                    if palette.dark {
                        rgb(0x67e8f9)
                    } else {
                        rgb(0x0e7490)
                    }
                } else {
                    scale_alpha(palette.window_border, if palette.dark { 0.85 } else { 1.0 })
                };
                let full_bg = if full_active {
                    if palette.dark {
                        rgb(0x22d3ee)
                    } else {
                        rgb(0x0891b2)
                    }
                } else {
                    scale_alpha(palette.row_hover_bg, if palette.dark { 0.95 } else { 1.0 })
                };
                let full_border = if full_active {
                    if palette.dark {
                        rgb(0x67e8f9)
                    } else {
                        rgb(0x0e7490)
                    }
                } else {
                    scale_alpha(palette.window_border, if palette.dark { 0.85 } else { 1.0 })
                };

                selector_header = selector_header.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .id(("parameter-mode-guided", item_id as u64))
                                .text_xs()
                                .text_color(if guided_active {
                                    if palette.dark {
                                        rgb(0x042f2e)
                                    } else {
                                        rgb(0xffffff)
                                    }
                                } else if has_structured_candidates {
                                    palette.row_text
                                } else {
                                    palette.muted_text
                                })
                                .bg(guided_bg)
                                .border_1()
                                .border_color(guided_border)
                                .rounded_md()
                                .px_1()
                                .py(px(1.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_parameter_editor_full_mode(false, cx);
                                }))
                                .child("Guided (g)"),
                        )
                        .child(
                            div()
                                .id(("parameter-mode-full", item_id as u64))
                                .text_xs()
                                .text_color(if full_active {
                                    if palette.dark {
                                        rgb(0x042f2e)
                                    } else {
                                        rgb(0xffffff)
                                    }
                                } else {
                                    palette.row_text
                                })
                                .bg(full_bg)
                                .border_1()
                                .border_color(full_border)
                                .rounded_md()
                                .px_1()
                                .py(px(1.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_parameter_editor_full_mode(true, cx);
                                }))
                                .child("Full (f)"),
                        ),
                );

                content = content.child(
                    div()
                        .w_full()
                        .p_2()
                        .bg(scale_alpha(
                            palette.row_hover_bg,
                            if palette.dark { 0.95 } else { 1.0 },
                        ))
                        .border_1()
                        .border_color(palette.selected_border)
                        .rounded_lg()
                        .child(selector_header)
                        .child(token_picker)
                        .child(
                            div()
                                .w_full()
                                .mt_1()
                                .text_xs()
                                .text_color(palette.muted_text)
                                .child(if self.parameter_editor_selected_targets.is_empty() {
                                    if auto_named_candidates {
                                        "pick one or more fields"
                                    } else {
                                        if cfg!(target_os = "macos") {
                                            "pick values • ⌘+click to split"
                                        } else {
                                            "pick values • Ctrl+click to split"
                                        }
                                    }
                                } else if auto_named_candidates {
                                    if cfg!(target_os = "macos") {
                                        "Enter saves • ⌘+click toggles"
                                    } else {
                                        "Enter saves • Ctrl+click toggles"
                                    }
                                } else {
                                    if cfg!(target_os = "macos") {
                                        "Enter then name • ⌘+click splits or toggles"
                                    } else {
                                        "Enter then name • Ctrl+click splits or toggles"
                                    }
                                }),
                        ),
                );
            } else {
                let parameter_name_focus_handle =
                    self.text_input_focus_handle(TextInputTarget::ParameterName);
                let mut name_rows = div().w_full().mt_1().flex().flex_col().gap_1();
                if self.parameter_editor_selected_targets.is_empty() {
                    name_rows = name_rows.child(
                        div()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child("No targets selected."),
                    );
                } else {
                    for (ix, target) in self.parameter_editor_selected_targets.iter().enumerate() {
                        let is_focus = ix == self.parameter_editor_name_focus_index;
                        let value = self
                            .parameter_editor_name_inputs
                            .get(ix)
                            .cloned()
                            .unwrap_or_default();
                        let value_display = if value.is_empty() {
                            "name".to_owned()
                        } else {
                            value
                        };
                        let value_color = if value_display == "name" {
                            palette.query_placeholder
                        } else {
                            palette.query_active
                        };
                        let mut name_input = div()
                            .w_full()
                            .mt_1()
                            .px_1()
                            .rounded_sm()
                            .line_height(px(22.0))
                            .text_sm()
                            .font_weight(FontWeight::NORMAL);
                        if is_focus {
                            name_input = name_input
                                .key_context("PastaTextInput")
                                .track_focus(&parameter_name_focus_handle)
                                .cursor(CursorStyle::IBeam)
                                .on_action(cx.listener(Self::query_backspace))
                                .on_action(cx.listener(Self::query_left))
                                .on_action(cx.listener(Self::query_right))
                                .on_action(cx.listener(Self::query_select_left))
                                .on_action(cx.listener(Self::query_select_right))
                                .on_action(cx.listener(Self::query_select_all))
                                .on_action(cx.listener(Self::query_home))
                                .on_action(cx.listener(Self::query_end))
                                .on_action(cx.listener(Self::query_show_character_palette))
                                .on_action(cx.listener(Self::query_paste))
                                .on_action(cx.listener(Self::query_cut))
                                .on_action(cx.listener(Self::query_copy))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, event, window, cx| {
                                        this.text_input_on_mouse_down(
                                            TextInputTarget::ParameterName,
                                            event,
                                            window,
                                            cx,
                                        );
                                    }),
                                )
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(|this, event, window, cx| {
                                        this.text_input_on_mouse_up(
                                            TextInputTarget::ParameterName,
                                            event,
                                            window,
                                            cx,
                                        );
                                    }),
                                )
                                .on_mouse_up_out(
                                    MouseButton::Left,
                                    cx.listener(|this, event, window, cx| {
                                        this.text_input_on_mouse_up(
                                            TextInputTarget::ParameterName,
                                            event,
                                            window,
                                            cx,
                                        );
                                    }),
                                )
                                .on_mouse_move(cx.listener(|this, event, window, cx| {
                                    this.text_input_on_mouse_move(
                                        TextInputTarget::ParameterName,
                                        event,
                                        window,
                                        cx,
                                    );
                                }))
                                .bg(scale_alpha(
                                    palette.selected_bg,
                                    if palette.dark { 0.95 } else { 0.75 },
                                ))
                                .border_1()
                                .border_color(palette.selected_border);
                        }

                        name_rows = name_rows.child(
                            div()
                                .id(("parameter-name-field", ix as u64))
                                .w_full()
                                .p_1()
                                .rounded_md()
                                .bg(if is_focus {
                                    scale_alpha(
                                        palette.selected_bg,
                                        if palette.dark { 0.75 } else { 0.45 },
                                    )
                                } else {
                                    scale_alpha(
                                        palette.row_hover_bg,
                                        if palette.dark { 0.92 } else { 1.0 },
                                    )
                                })
                                .border_1()
                                .border_color(if is_focus {
                                    palette.selected_border
                                } else {
                                    scale_alpha(
                                        palette.window_border,
                                        if palette.dark { 0.88 } else { 1.0 },
                                    )
                                })
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.focus_parameter_name_index(ix, cx);
                                }))
                                .child(
                                    div()
                                        .w_full()
                                        .text_xs()
                                        .text_color(palette.muted_text)
                                        .child(target.clone()),
                                )
                                .child(if is_focus {
                                    name_input.child(TextInputElement::new(
                                        cx.entity(),
                                        TextInputTarget::ParameterName,
                                        "name",
                                        palette,
                                        true,
                                    ))
                                } else {
                                    div()
                                        .w_full()
                                        .mt_1()
                                        .text_sm()
                                        .text_color(value_color)
                                        .child(value_display)
                                }),
                        );
                    }
                }

                content = content.child(
                    div()
                        .w_full()
                        .p_2()
                        .bg(scale_alpha(
                            palette.row_hover_bg,
                            if palette.dark { 0.95 } else { 1.0 },
                        ))
                        .border_1()
                        .border_color(palette.selected_border)
                        .rounded_lg()
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(palette.title_text)
                                        .child(format!("Parameter Name • Snippet #{item_id}")),
                                ),
                        )
                        .child(name_rows),
                );
            }
        }

        if let Some(item_id) = self.parameter_fill_target_id {
            let parameters = self
                .items
                .iter()
                .find(|entry| entry.id == item_id)
                .map(|entry| entry.parameters.clone())
                .unwrap_or_default();
            let parameter_fill_focus_handle =
                self.text_input_focus_handle(TextInputTarget::ParameterFill);
            let mut fill_rows = div().w_full().mt_1().flex().flex_col().gap_1();
            for (ix, parameter) in parameters.iter().enumerate() {
                let is_focus = ix == self.parameter_fill_focus_index;
                let value = self
                    .parameter_fill_values
                    .get(ix)
                    .cloned()
                    .unwrap_or_default();
                let value_display = if value.is_empty() {
                    "Type value…".to_owned()
                } else {
                    value
                };
                let value_color = if value_display == "Type value…" {
                    palette.query_placeholder
                } else {
                    palette.query_active
                };
                let mut fill_input = div()
                    .w_full()
                    .mt_1()
                    .px_1()
                    .rounded_sm()
                    .line_height(px(22.0))
                    .text_sm()
                    .font_weight(FontWeight::NORMAL);
                if is_focus {
                    fill_input = fill_input
                        .key_context("PastaTextInput")
                        .track_focus(&parameter_fill_focus_handle)
                        .cursor(CursorStyle::IBeam)
                        .on_action(cx.listener(Self::query_backspace))
                        .on_action(cx.listener(Self::query_left))
                        .on_action(cx.listener(Self::query_right))
                        .on_action(cx.listener(Self::query_select_left))
                        .on_action(cx.listener(Self::query_select_right))
                        .on_action(cx.listener(Self::query_select_all))
                        .on_action(cx.listener(Self::query_home))
                        .on_action(cx.listener(Self::query_end))
                        .on_action(cx.listener(Self::query_show_character_palette))
                        .on_action(cx.listener(Self::query_paste))
                        .on_action(cx.listener(Self::query_cut))
                        .on_action(cx.listener(Self::query_copy))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event, window, cx| {
                                this.text_input_on_mouse_down(
                                    TextInputTarget::ParameterFill,
                                    event,
                                    window,
                                    cx,
                                );
                            }),
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, event, window, cx| {
                                this.text_input_on_mouse_up(
                                    TextInputTarget::ParameterFill,
                                    event,
                                    window,
                                    cx,
                                );
                            }),
                        )
                        .on_mouse_up_out(
                            MouseButton::Left,
                            cx.listener(|this, event, window, cx| {
                                this.text_input_on_mouse_up(
                                    TextInputTarget::ParameterFill,
                                    event,
                                    window,
                                    cx,
                                );
                            }),
                        )
                        .on_mouse_move(cx.listener(|this, event, window, cx| {
                            this.text_input_on_mouse_move(
                                TextInputTarget::ParameterFill,
                                event,
                                window,
                                cx,
                            );
                        }))
                        .bg(scale_alpha(
                            palette.selected_bg,
                            if palette.dark { 0.95 } else { 0.75 },
                        ))
                        .border_1()
                        .border_color(palette.selected_border);
                }

                fill_rows = fill_rows.child(
                    div()
                        .id(("parameter-fill-field", ix as u64))
                        .w_full()
                        .p_1()
                        .rounded_md()
                        .bg(if is_focus {
                            scale_alpha(palette.selected_bg, if palette.dark { 0.78 } else { 0.48 })
                        } else {
                            scale_alpha(palette.row_hover_bg, if palette.dark { 0.92 } else { 1.0 })
                        })
                        .border_1()
                        .border_color(if is_focus {
                            palette.selected_border
                        } else {
                            scale_alpha(
                                palette.window_border,
                                if palette.dark { 0.88 } else { 1.0 },
                            )
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.focus_parameter_fill_index(ix, cx);
                        }))
                        .child(
                            div()
                                .w_full()
                                .text_xs()
                                .text_color(palette.muted_text)
                                .child(parameter.name.clone()),
                        )
                        .child(if is_focus {
                            fill_input.child(TextInputElement::new(
                                cx.entity(),
                                TextInputTarget::ParameterFill,
                                "Type value…",
                                palette,
                                true,
                            ))
                        } else {
                            div()
                                .w_full()
                                .mt_1()
                                .text_sm()
                                .text_color(value_color)
                                .child(value_display)
                        }),
                );
            }
            if parameters.is_empty() {
                fill_rows = fill_rows.child(
                    div()
                        .text_xs()
                        .text_color(palette.muted_text)
                        .child("No parameters found."),
                );
            }

            content = content.child(
                div()
                    .w_full()
                    .p_2()
                    .bg(scale_alpha(
                        palette.row_hover_bg,
                        if palette.dark { 0.95 } else { 1.0 },
                    ))
                    .border_1()
                    .border_color(palette.selected_border)
                    .rounded_lg()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(palette.title_text)
                                    .child(format!("Fill Parameters • Snippet #{item_id}")),
                            ),
                    )
                    .child(fill_rows)
                    .child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child("blank all = original"),
                    ),
            );
        }

        if self.transform_menu_open {
            let mut transform_buttons = div()
                .w_full()
                .mt_1()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_1();
            for (ix, (label, action)) in [
                ("s  Shell quote", TransformAction::ShellQuote),
                ("j  JSON encode", TransformAction::JsonEncode),
                ("J  JSON decode", TransformAction::JsonDecode),
                ("f  JSON pretty", TransformAction::JsonPretty),
                ("F  JSON minify", TransformAction::JsonMinify),
                ("u  URL encode", TransformAction::UrlEncode),
                ("U  URL decode", TransformAction::UrlDecode),
                ("b  Base64 encode", TransformAction::Base64Encode),
                ("B  Base64 decode", TransformAction::Base64Decode),
                ("t  JWT decode", TransformAction::JwtDecode),
                ("e  Epoch decode", TransformAction::EpochDecode),
                ("h  SHA256 hash", TransformAction::Sha256Hash),
                ("c  Count stats", TransformAction::ContentStats),
                ("p  Cert info", TransformAction::PublicCertPemInfo),
            ]
            .into_iter()
            .enumerate()
            {
                let button_bg =
                    scale_alpha(palette.row_hover_bg, if palette.dark { 0.95 } else { 1.0 });
                let button_border =
                    scale_alpha(palette.window_border, if palette.dark { 0.9 } else { 1.0 });
                let button_hover =
                    scale_alpha(palette.selected_bg, if palette.dark { 0.95 } else { 1.0 });
                transform_buttons = transform_buttons.child(
                    div()
                        .id(("transform-action", ix as u64))
                        .flex_none()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .px(px(4.0))
                        .py(px(1.0))
                        .rounded_sm()
                        .bg(button_bg)
                        .border_1()
                        .border_color(button_border)
                        .text_size(px(10.0))
                        .line_height(px(14.0))
                        .text_color(palette.row_text)
                        .hover(move |style| style.bg(button_hover))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.apply_transform_action(action, cx);
                        }))
                        .child(label),
                );
            }

            content = content.child(
                div()
                    .w_full()
                    .p_2()
                    .bg(scale_alpha(
                        palette.row_hover_bg,
                        if palette.dark { 0.95 } else { 1.0 },
                    ))
                    .border_1()
                    .border_color(palette.selected_border)
                    .rounded_lg()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(palette.title_text)
                                    .child("Transforms"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(palette.muted_text)
                                    .child("Type shortcut or click"),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child("Tab/Esc cancel"),
                    )
                    .child(transform_buttons),
            );
        }

        let workspace = div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .flex()
            .gap_2()
            .child(
                div()
                    .w(relative(RESULTS_LIST_WIDTH_RATIO))
                    .h_full()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .child(results),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w(px(0.0))
                    .py(px(4.0))
                    .child(self.render_preview_pane(palette)),
            );

        content
            .child(div().w_full().h(px(1.0)).bg(palette.list_divider))
            .child(workspace)
            .child(if self.show_command_help {
                div()
                    .w_full()
                    .flex_none()
                    .p_2()
                    .bg(scale_alpha(
                        palette.row_hover_bg,
                        if palette.dark { 0.95 } else { 1.0 },
                    ))
                    .border_1()
                    .border_color(scale_alpha(
                        palette.window_border,
                        if palette.dark { 0.9 } else { 1.0 },
                    ))
                    .rounded_lg()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.title_text)
                            .child("Commands"),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_start()
                            .gap_2()
                            .child(render_help_run(
                                if cfg!(target_os = "macos") {
                                    &["⏎ copy", "⌘R reveal secret", "⌘J / ⌘K / ⌘L / ⌘; navigate"]
                                } else {
                                    &["⏎ copy", "Ctrl+R reveal secret", "Ctrl+J/K/L/; navigate"]
                                },
                                palette,
                            ))
                            .child(render_help_run(
                                if cfg!(target_os = "macos") {
                                    &["⌘I edit info", "⌘P parametrize", "Tab transforms"]
                                } else {
                                    &["Ctrl+I edit info", "Ctrl+P parametrize", "Tab transforms"]
                                },
                                palette,
                            ))
                            .child(render_help_run(
                                if cfg!(target_os = "macos") {
                                    &[
                                        "⌘T add tags",
                                        "⌘⇧T remove tags",
                                        "⌘B set bowl",
                                        "⌘⇧B remove bowl",
                                        "⌥⌘B import bowl",
                                    ]
                                } else {
                                    &[
                                        "Ctrl+T add tags",
                                        "Ctrl+Shift+T remove tags",
                                        "Ctrl+B set bowl",
                                        "Ctrl+Shift+B remove bowl",
                                        "Ctrl+Alt+B import bowl",
                                    ]
                                },
                                palette,
                            ))
                            .child(render_help_run(
                                &[":b search bowl", ":e export bowl", "↹ autocomplete"],
                                palette,
                            ))
                            .child(render_help_run(
                                if cfg!(target_os = "macos") {
                                    &[
                                        "⌘⇧S toggle secret",
                                        "⌘D delete",
                                        "Esc close",
                                        "⌘Q quit",
                                        "⌘H hide help",
                                    ]
                                } else {
                                    &[
                                        "Ctrl+Shift+S toggle secret",
                                        "Ctrl+D delete",
                                        "Esc close",
                                        "Ctrl+Q quit",
                                        "Ctrl+H hide help",
                                    ]
                                },
                                palette,
                            )),
                    )
            } else {
                div()
                    .w_full()
                    .text_xs()
                    .text_color(palette.muted_text)
                    .child(if cfg!(target_os = "macos") {
                        "⌘H commands"
                    } else {
                        "Ctrl+H commands"
                    })
            })
    }
}

impl LauncherView {
    fn render_tag_search_suggestions(
        &self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut chips = div().w_full().flex().flex_row().flex_wrap().gap_1();
        for (ix, suggestion) in self.tag_search_suggestions.iter().enumerate() {
            let is_primary = ix == 0;
            let chip_bg = if is_primary {
                scale_alpha(palette.selected_bg, if palette.dark { 0.92 } else { 0.72 })
            } else {
                scale_alpha(palette.row_hover_bg, if palette.dark { 0.9 } else { 1.0 })
            };
            let chip_border = if is_primary {
                palette.selected_border
            } else {
                scale_alpha(palette.window_border, if palette.dark { 0.84 } else { 0.9 })
            };
            let chip_text = if is_primary {
                palette.title_text
            } else {
                palette.muted_text
            };

            chips = chips.child(
                div()
                    .id(("tag-search-suggestion", ix))
                    .text_xs()
                    .text_color(chip_text)
                    .bg(chip_bg)
                    .border_1()
                    .border_color(chip_border)
                    .rounded_md()
                    .px_1()
                    .py(px(1.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_tag_search_suggestion_index(ix, cx);
                    }))
                    .child(search_suggestion_label(&self.query, suggestion)),
            );
        }

        div()
            .w_full()
            .mt_1()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child(search_suggestion_heading(&self.query)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child("↹ autocomplete"),
                    ),
            )
            .child(chips)
            .into_any_element()
    }

    fn render_preview_pane(&self, palette: Palette) -> AnyElement {
        let mut pane = div()
            .w_full()
            .h_full()
            .min_w(px(0.0))
            .p_2()
            .bg(scale_alpha(
                palette.row_hover_bg,
                if palette.dark { 0.92 } else { 1.0 },
            ))
            .border_1()
            .border_color(scale_alpha(
                palette.window_border,
                if palette.dark { 0.9 } else { 1.0 },
            ))
            .rounded_lg()
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap_2();

        let Some(item) = self.items.get(self.selected_index) else {
            return pane
                .items_center()
                .justify_center()
                .text_center()
                .child(div().text_sm().text_color(palette.muted_text).child(
                    if self.query.is_empty() {
                        "Nothing to inspect."
                    } else {
                        "No matches."
                    },
                ))
                .into_any_element();
        };

        let Some(row_data) = self.row_presentations.get(self.selected_index) else {
            return pane.into_any_element();
        };

        let is_masked_secret =
            item.item_type == ClipboardItemType::Password && self.is_secret_masked(item.id);
        let preview_settled = Instant::now().duration_since(self.selection_changed_at)
            >= Duration::from_millis(PREVIEW_SETTLE_DELAY_MS);
        let preview_language = if is_masked_secret {
            None
        } else {
            row_data.detected_language
        };
        let preview_text = if is_masked_secret {
            row_data.masked_preview.clone()
        } else if preview_settled {
            row_data.expanded_preview.clone()
        } else {
            row_data.collapsed_preview.clone()
        };
        let created_detail = format_timestamp_detail(&item.created_at);
        let primary_action_hint = if is_masked_secret {
            if cfg!(target_os = "macos") {
                "⌘R Reveal"
            } else {
                "Ctrl+R Reveal"
            }
        } else {
            "⏎ Copy"
        };
        let secret_action_hint = if item.item_type == ClipboardItemType::Password {
            if cfg!(target_os = "macos") {
                "⌘⇧S Unmark secret"
            } else {
                "Ctrl+Shift+S Unmark secret"
            }
        } else {
            if cfg!(target_os = "macos") {
                "⌘⇧S Mark secret"
            } else {
                "Ctrl+Shift+S Mark secret"
            }
        };
        let preview_syntax_enabled = self.syntax_highlighting
            && self.query.trim().is_empty()
            && !is_masked_secret
            && preview_settled
            && row_data.expanded_preview.len() <= PREVIEW_PANE_SYNTAX_MAX_CHARS
            && row_data.expanded_preview_line_count <= PREVIEW_PANE_SYNTAX_MAX_LINES;

        let mut item_tags = row_data.base_tags.clone();
        if item.item_type == ClipboardItemType::Password {
            if let Some(seconds) = self.secret_seconds_left(item.id) {
                item_tags.insert(0, format!("OPEN {seconds}s"));
            } else {
                item_tags.insert(0, "LOCKED".to_owned());
            }
        }

        let mut tag_row = div().w_full().flex().flex_row().flex_wrap().gap_1();
        for tag in item_tags.iter() {
            tag_row = tag_row.child(
                div()
                    .text_size(px(10.0))
                    .line_height(px(14.0))
                    .text_color(tag_chip_color(tag, palette.dark))
                    .bg(scale_alpha(
                        palette.row_hover_bg,
                        if palette.dark { 0.96 } else { 1.0 },
                    ))
                    .border_1()
                    .border_color(scale_alpha(
                        palette.window_border,
                        if palette.dark { 0.9 } else { 1.0 },
                    ))
                    .rounded_sm()
                    .px(px(4.0))
                    .child(tag.clone()),
            );
        }

        pane = pane
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_between()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .gap_1()
                            .child(result_meta_chip(primary_action_hint, palette))
                            .child(result_meta_chip(secret_action_hint, palette)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.row_meta_text)
                            .child(created_detail),
                    ),
            )
            .child(tag_row);

        if !item.description.trim().is_empty() {
            pane = pane.child(
                div()
                    .w_full()
                    .p_2()
                    .bg(scale_alpha(
                        palette.selected_bg,
                        if palette.dark { 0.65 } else { 0.38 },
                    ))
                    .rounded_md()
                    .child(div().text_xs().text_color(palette.muted_text).child("Info"))
                    .child(
                        div()
                            .mt_1()
                            .text_sm()
                            .text_color(palette.row_text)
                            .child(item.description.clone()),
                    ),
            );
        }

        if !item.parameters.is_empty() {
            let mut parameter_row = div().w_full().mt_1().flex().flex_row().flex_wrap().gap_1();
            for parameter in item.parameters.iter().take(8) {
                parameter_row = parameter_row.child(
                    div()
                        .text_xs()
                        .text_color(palette.row_text)
                        .bg(scale_alpha(
                            palette.row_hover_bg,
                            if palette.dark { 0.95 } else { 1.0 },
                        ))
                        .border_1()
                        .border_color(scale_alpha(
                            palette.window_border,
                            if palette.dark { 0.9 } else { 1.0 },
                        ))
                        .rounded_md()
                        .px_1()
                        .child(parameter.name.clone()),
                );
            }

            pane = pane.child(
                div()
                    .w_full()
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child(format!("Parameters ({})", item.parameters.len())),
                    )
                    .child(parameter_row),
            );
        }

        if row_data.expanded_preview_truncated {
            pane = pane.child(
                div()
                    .w_full()
                    .text_xs()
                    .text_color(palette.muted_text)
                    .child("Preview shortened for speed."),
            );
        }

        pane.child(div().w_full().h(px(1.0)).bg(palette.list_divider))
            .child(
                div()
                    .id(("preview-scroll", item.id as u64))
                    .w_full()
                    .flex_1()
                    .overflow_y_scroll()
                    .pr_2()
                    .child(div().w_full().text_sm().text_color(palette.row_text).child(
                        syntax_styled_text(
                            &preview_text,
                            preview_language,
                            preview_syntax_enabled,
                            palette.dark,
                        ),
                    )),
            )
            .into_any_element()
    }

    fn render_result_row(
        &self,
        ix: usize,
        item: &ClipboardRecord,
        row_data: &CachedRowPresentation,
        palette: Palette,
        info_editor_open: bool,
        tag_editor_open: bool,
        bowl_editor_open: bool,
        parameter_editor_open: bool,
        parameter_fill_open: bool,
        transform_menu_open: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_selected = ix == self.selected_index;
        let mut item_tags = row_data.base_tags.clone();
        if item.item_type == ClipboardItemType::Password {
            if let Some(seconds) = self.secret_seconds_left(item.id) {
                item_tags.insert(0, format!("OPEN {seconds}s"));
            } else {
                item_tags.insert(0, "LOCKED".to_owned());
            }
        }
        let is_masked_secret =
            item.item_type == ClipboardItemType::Password && self.is_secret_masked(item.id);
        let item_preview = if is_masked_secret {
            row_data.masked_preview.clone()
        } else {
            row_data.collapsed_preview.clone()
        };
        let preview_language = if is_masked_secret {
            None
        } else {
            row_data.detected_language
        };
        let row_syntax_enabled = false;

        let default_row_bg =
            scale_alpha(palette.row_hover_bg, if palette.dark { 0.62 } else { 0.92 });
        let default_row_border = scale_alpha(
            palette.window_border,
            if palette.dark { 0.78 } else { 0.88 },
        );

        let mut row = div()
            .id(("result", item.id as u64))
            .w_full()
            .h_full()
            .p_1()
            .rounded_lg()
            .overflow_hidden()
            .bg(if is_selected {
                palette.selected_bg
            } else {
                default_row_bg
            })
            .border_1()
            .border_color(if is_selected {
                palette.selected_border
            } else {
                default_row_border
            });
        if !info_editor_open
            && !tag_editor_open
            && !bowl_editor_open
            && !parameter_editor_open
            && !parameter_fill_open
            && !transform_menu_open
        {
            row = row
                .hover({
                    let row_hover = palette.row_hover_bg;
                    move |style| style.bg(row_hover)
                })
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.select_result_index(ix, cx);
                }));
        }

        let mut tag_row = div().flex().items_center().gap_1().overflow_hidden();
        for tag in item_tags.iter() {
            tag_row = tag_row.child(result_meta_chip(tag, palette));
        }

        let tag_lane = div()
            .w(relative(0.5))
            .min_w(px(0.0))
            .overflow_hidden()
            .child(tag_row);

        let mut right_meta = div().w_full().flex().items_center().justify_end().gap_1();
        if let Some(bowl_name) = &row_data.bowl_name {
            right_meta = right_meta.child(
                div()
                    .max_w(relative(0.68))
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_end()
                            .child(result_meta_chip(&format!("BOWL:{bowl_name}"), palette)),
                    ),
            );
        }
        right_meta = right_meta.child(
            div()
                .flex_none()
                .text_xs()
                .text_color(palette.row_meta_text)
                .child(row_data.created_label.clone()),
        );

        let top_row = div()
            .w_full()
            .flex()
            .items_center()
            .child(tag_lane)
            .child(div().w(relative(0.5)).min_w(px(0.0)).child(right_meta));
        row = row.child(top_row);

        let preview_block = div()
            .w_full()
            .mt_1()
            .text_sm()
            .text_color(palette.row_text)
            .whitespace_normal()
            .line_clamp(4);
        row = row.child(preview_block.child(syntax_styled_text(
            &item_preview,
            preview_language,
            row_syntax_enabled,
            palette.dark,
        )));

        div()
            .w_full()
            .h(px(RESULT_ROW_HEIGHT))
            .py(px(4.0))
            .child(row)
            .into_any_element()
    }
}

fn search_suggestion_heading(query: &str) -> &'static str {
    match parse_search_query(query) {
        SearchQuery::TagOnly { .. } => "Tag suggestions",
        SearchQuery::Bowl { .. } | SearchQuery::ExportBowl { .. } => "Bowl suggestions",
        SearchQuery::Default { .. } => "Suggestions",
    }
}

fn search_suggestion_label(query: &str, suggestion: &str) -> String {
    match parse_search_query(query) {
        SearchQuery::TagOnly { .. } => format!(":{suggestion}"),
        SearchQuery::Bowl { .. } => format!(":b {suggestion}"),
        SearchQuery::ExportBowl { .. } => format!(":e {suggestion}"),
        SearchQuery::Default { .. } => suggestion.to_owned(),
    }
}

fn result_meta_chip(label: &str, palette: Palette) -> impl IntoElement {
    div()
        .flex_none()
        .text_size(px(10.0))
        .line_height(px(14.0))
        .text_color(tag_chip_color(label, palette.dark))
        .bg(scale_alpha(
            palette.row_hover_bg,
            if palette.dark { 0.95 } else { 1.0 },
        ))
        .border_1()
        .border_color(scale_alpha(
            palette.window_border,
            if palette.dark { 0.85 } else { 1.0 },
        ))
        .rounded_sm()
        .px(px(4.0))
        .whitespace_nowrap()
        .child(label.to_owned())
}

fn render_help_run(tips: &[&str], palette: Palette) -> impl IntoElement {
    let help_chip_bg = scale_alpha(palette.row_hover_bg, if palette.dark { 0.9 } else { 1.0 });
    let help_chip_border =
        scale_alpha(palette.window_border, if palette.dark { 0.84 } else { 0.9 });

    let mut chips = div().flex().flex_row().flex_wrap().items_center().gap_1();
    for tip in tips {
        chips = chips.child(
            div()
                .text_xs()
                .text_color(palette.muted_text)
                .bg(help_chip_bg)
                .border_1()
                .border_color(help_chip_border)
                .rounded_md()
                .px_1()
                .py(px(1.0))
                .child((*tip).to_owned()),
        );
    }

    div().flex_none().max_w_full().child(chips)
}
