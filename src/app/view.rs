#[cfg(target_os = "macos")]
use super::actions::{has_structured_parameter_candidates, parameter_clickable_candidates};
#[cfg(target_os = "macos")]
use super::query_input::QueryInputElement;
#[cfg(target_os = "macos")]
use super::state::CachedRowPresentation;
#[cfg(target_os = "macos")]
use crate::*;
#[cfg(target_os = "macos")]
use gpui::AnyElement;

#[cfg(target_os = "macos")]
impl Render for LauncherView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette_for(window.appearance(), self.surface_alpha);
        let info_editor_open = self.info_editor_target_id.is_some();
        let tag_editor_open = self.tag_editor_target_id.is_some();
        let parameter_editor_open = self.parameter_editor_target_id.is_some();
        let parameter_fill_open = self.parameter_fill_target_id.is_some();
        let transform_menu_open = self.transform_menu_open;
        let query_input_enabled = self.query_input_enabled();
        let query_focused = self.query_focus_handle.is_focused(window);
        let target_height = LAUNCHER_HEIGHT;
        if (self.window_height_target - target_height).abs() > WINDOW_HEIGHT_ANIMATION_SNAP {
            self.window_height_from = self.window_height;
            self.window_height_target = target_height;
            self.window_height_started_at = Instant::now();
            self.window_height_duration =
                Duration::from_millis(WINDOW_HEIGHT_ANIMATION_DURATION_MS);
        }
        let results_height = RESULTS_HEIGHT_NORMAL;

        let results = if self.items.is_empty() {
            div()
                .id("results-list")
                .w_full()
                .h(px(results_height))
                .flex()
                .items_center()
                .justify_center()
                .text_color(palette.muted_text)
                .text_sm()
                .child("Clipboard is empty. Copy text/code/commands to get started.")
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
            .h(px(results_height))
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
                            .child("⌥ + SPACE"),
                    ),
            )
            .child({
                let mut query_container = div()
                    .w_full()
                    .px_1()
                    .rounded_md()
                    .line_height(px(28.0))
                    .text_lg()
                    .font_weight(FontWeight::NORMAL);

                if query_input_enabled {
                    query_container = query_container
                        .key_context("PastaQueryInput")
                        .track_focus(&self.query_focus_handle)
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
                        .on_mouse_down(MouseButton::Left, cx.listener(Self::query_on_mouse_down))
                        .on_mouse_up(MouseButton::Left, cx.listener(Self::query_on_mouse_up))
                        .on_mouse_up_out(MouseButton::Left, cx.listener(Self::query_on_mouse_up))
                        .on_mouse_move(cx.listener(Self::query_on_mouse_move));
                }

                if query_focused && query_input_enabled {
                    query_container = query_container
                        .bg(scale_alpha(
                            palette.selected_bg,
                            if palette.dark { 0.95 } else { 0.75 },
                        ))
                        .border_1()
                        .border_color(palette.selected_border);
                }

                div()
                    .w_full()
                    .child(query_container.child(QueryInputElement::new(
                        cx.entity(),
                        palette,
                        query_input_enabled,
                    )))
            });

        if let Some(item_id) = self.info_editor_target_id {
            let info_display = if self.info_editor_input.is_empty() {
                "Add info…".to_owned()
            } else {
                self.info_editor_input.clone()
            };
            let info_color = if self.info_editor_input.is_empty() {
                palette.query_placeholder
            } else {
                palette.query_active
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
                                    .child(format!("Snippet Info • Snippet #{item_id}")),
                            ),
                    )
                    .child(
                        if self.info_editor_select_all && !self.info_editor_input.is_empty() {
                            div()
                                .w_full()
                                .mt_1()
                                .text_sm()
                                .text_color(info_color)
                                .bg(scale_alpha(
                                    palette.selected_bg,
                                    if palette.dark { 0.92 } else { 0.68 },
                                ))
                                .rounded_sm()
                                .child(info_display)
                        } else {
                            div()
                                .w_full()
                                .mt_1()
                                .text_sm()
                                .text_color(info_color)
                                .child(info_display)
                        },
                    )
                    .child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child("⌘V paste"),
                    ),
            );
        }

        if let Some(item_id) = self.tag_editor_target_id {
            let input_display = if self.tag_editor_input.is_empty() {
                "tag1,tag2".to_owned()
            } else {
                self.tag_editor_input.clone()
            };
            let input_color = if self.tag_editor_input.is_empty() {
                palette.query_placeholder
            } else {
                palette.query_active
            };
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
                    .child(
                        if self.tag_editor_select_all && !self.tag_editor_input.is_empty() {
                            div()
                                .w_full()
                                .mt_1()
                                .text_sm()
                                .text_color(input_color)
                                .bg(scale_alpha(
                                    palette.selected_bg,
                                    if palette.dark { 0.92 } else { 0.68 },
                                ))
                                .rounded_sm()
                                .child(input_display)
                        } else {
                            div()
                                .w_full()
                                .mt_1()
                                .text_sm()
                                .text_color(input_color)
                                .child(input_display)
                        },
                    )
                    .child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child("comma-separated • ⌘V"),
                    ),
            );
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
                        scale_alpha(
                            palette.row_hover_bg,
                            if palette.dark { 0.92 } else { 1.0 },
                        )
                    };
                    let chip_border = if is_selected {
                        if palette.dark {
                            rgb(0x67e8f9)
                        } else {
                            rgb(0x0e7490)
                        }
                    } else {
                        scale_alpha(
                            palette.window_border,
                            if palette.dark { 0.85 } else { 1.0 },
                        )
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
                                let additive = event.modifiers().platform;
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
                                        "pick one or more values"
                                    }
                                } else if auto_named_candidates {
                                    "Enter saves • ⌘+click toggles"
                                } else {
                                    "Enter then name • ⌘+click toggles"
                                }),
                        ),
                );
            } else {
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
                        let value = self
                            .parameter_editor_name_inputs
                            .get(ix)
                            .cloned()
                            .unwrap_or_default();
                        let value_is_empty = value.is_empty();
                        let is_focus = ix == self.parameter_editor_name_focus_index;
                        let value_display = if value_is_empty {
                            "name".to_owned()
                        } else {
                            value
                        };
                        let value_color = if value_display == "name" {
                            palette.query_placeholder
                        } else {
                            palette.query_active
                        };
                        let selected_all_value =
                            is_focus && self.parameter_editor_name_select_all && !value_is_empty;

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
                                .child(
                                    div()
                                        .w_full()
                                        .mt_1()
                                        .text_sm()
                                        .text_color(value_color)
                                        .bg(if selected_all_value {
                                            scale_alpha(
                                                palette.selected_bg,
                                                if palette.dark { 0.92 } else { 0.68 },
                                            )
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .rounded_sm()
                                        .child(value_display),
                                ),
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
            let mut fill_rows = div().w_full().mt_1().flex().flex_col().gap_1();
            for (ix, parameter) in parameters.iter().enumerate() {
                let value = self
                    .parameter_fill_values
                    .get(ix)
                    .cloned()
                    .unwrap_or_default();
                let value_is_empty = value.is_empty();
                let is_focus = ix == self.parameter_fill_focus_index;
                let value_display = if value_is_empty {
                    "Type value…".to_owned()
                } else {
                    value
                };
                let value_color = if value_display == "Type value…" {
                    palette.query_placeholder
                } else {
                    palette.query_active
                };
                let selected_all_value =
                    is_focus && self.parameter_fill_select_all && !value_is_empty;

                fill_rows = fill_rows.child(
                    div()
                        .id(("parameter-fill-field", ix as u64))
                        .w_full()
                        .p_1()
                        .rounded_md()
                        .bg(if is_focus {
                            scale_alpha(palette.selected_bg, if palette.dark { 0.78 } else { 0.48 })
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
                            this.focus_parameter_fill_index(ix, cx);
                        }))
                        .child(
                            div()
                                .w_full()
                                .text_xs()
                                .text_color(palette.muted_text)
                                .child(parameter.name.clone()),
                        )
                        .child(
                            div()
                                .w_full()
                                .mt_1()
                                .text_sm()
                                .text_color(value_color)
                                .bg(if selected_all_value {
                                    scale_alpha(
                                        palette.selected_bg,
                                        if palette.dark { 0.92 } else { 0.68 },
                                    )
                                } else {
                                    rgba(0x00000000)
                                })
                                .rounded_sm()
                                .child(value_display),
                        ),
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
                ("u  URL encode", TransformAction::UrlEncode),
                ("U  URL decode", TransformAction::UrlDecode),
                ("b  Base64 encode", TransformAction::Base64Encode),
                ("B  Base64 decode", TransformAction::Base64Decode),
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
                        .px_1()
                        .py(px(2.0))
                        .rounded_md()
                        .bg(button_bg)
                        .border_1()
                        .border_color(button_border)
                        .text_xs()
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

        content
            .child(div().w_full().h(px(1.0)).bg(palette.list_divider))
            .child(results)
            .child(
                if self.show_command_help {
                    div()
                        .w_full()
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
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(palette.title_text)
                                .child("Commands"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(palette.muted_text)
                                .child("Search • /tag tag-only • Enter copy • Tab transforms • ⌘R reveal+copy secret • ⌘I info • ⌘P parametrize"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(palette.muted_text)
                                .child("⌘J/⌘K/⌘L/⌘; navigate • click token chips to parametrize • ⌘⇧S mark secret • ⌘T add tags • ⌘⇧T remove tags • ⌘D delete • Esc close • ⌘Q close • ⌘H hide help"),
                        )
                } else {
                    div()
                        .w_full()
                        .text_xs()
                        .text_color(palette.muted_text)
                        .child("⌘H commands")
                },
            )
    }
}

#[cfg(target_os = "macos")]
impl LauncherView {
    fn render_result_row(
        &self,
        ix: usize,
        item: &ClipboardRecord,
        row_data: &CachedRowPresentation,
        palette: Palette,
        info_editor_open: bool,
        tag_editor_open: bool,
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
        let row_syntax_enabled = self.syntax_highlighting && is_selected;

        let mut row = div()
            .id(("result", item.id as u64))
            .w_full()
            .h(px(RESULT_ROW_HEIGHT))
            .p_1()
            .rounded_lg()
            .overflow_hidden()
            .bg(if is_selected {
                palette.selected_bg
            } else {
                rgba(0x00000000)
            });
        if !info_editor_open
            && !tag_editor_open
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
                    this.copy_index_to_clipboard(ix, cx);
                }));
        }

        let mut top_row = div().w_full().flex().justify_between().items_center();
        if !item_tags.is_empty() {
            let mut tags_row = div().flex().items_center().gap_1().overflow_hidden();
            for tag in item_tags.iter() {
                tags_row = tags_row.child(
                    div()
                        .text_xs()
                        .text_color(tag_chip_color(tag, palette.dark))
                        .bg(scale_alpha(
                            palette.row_hover_bg,
                            if palette.dark { 0.95 } else { 1.0 },
                        ))
                        .border_1()
                        .border_color(scale_alpha(
                            palette.window_border,
                            if palette.dark { 0.85 } else { 1.0 },
                        ))
                        .rounded_md()
                        .px_1()
                        .child(tag.clone()),
                );
            }
            top_row = top_row.child(tags_row);
        }
        top_row = top_row.child(
            div()
                .text_xs()
                .text_color(palette.row_meta_text)
                .child(row_data.created_label.clone()),
        );
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

        if is_selected {
            row = row.border_1().border_color(palette.selected_border);
        }

        row.into_any_element()
    }
}
