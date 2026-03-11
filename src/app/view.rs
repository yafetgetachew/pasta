#[cfg(target_os = "macos")]
use super::actions::parameter_clickable_ranges;
#[cfg(target_os = "macos")]
use crate::*;

#[cfg(target_os = "macos")]
impl Render for LauncherView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette_for(window.appearance(), self.surface_alpha);
        let query_display = if self.query.is_empty() {
            "Search snippets, commands, and passwords…".to_owned()
        } else {
            self.query.clone()
        };
        let query_color = if self.query.is_empty() {
            palette.query_placeholder
        } else {
            palette.query_active
        };
        let query_is_selected = self.query_select_all && !self.query.is_empty();
        let info_editor_open = self.info_editor_target_id.is_some();
        let tag_editor_open = self.tag_editor_target_id.is_some();
        let parameter_editor_open = self.parameter_editor_target_id.is_some();
        let parameter_fill_open = self.parameter_fill_target_id.is_some();
        let transform_menu_open = self.transform_menu_open;
        let selection_stable = Instant::now().duration_since(self.selection_changed_at)
            >= Duration::from_millis(SELECTION_EXPAND_DWELL_MS);
        let selected_should_expand = selection_stable
            && !info_editor_open
            && !tag_editor_open
            && !parameter_editor_open
            && !parameter_fill_open
            && !transform_menu_open
            && self.items.get(self.selected_index).is_some_and(|item| {
                !(item.item_type == ClipboardItemType::Password && self.is_secret_masked(item.id))
                    && preview_would_truncate(&item.content)
            });
        let target_height = if selected_should_expand {
            LAUNCHER_EXPANDED_HEIGHT
        } else {
            LAUNCHER_HEIGHT
        };
        if (self.window_height_target - target_height).abs() > WINDOW_HEIGHT_ANIMATION_SNAP {
            self.window_height_from = self.window_height;
            self.window_height_target = target_height;
            self.window_height_started_at = Instant::now();
            self.window_height_duration =
                Duration::from_millis(WINDOW_HEIGHT_ANIMATION_DURATION_MS);
        }
        let expansion_range = (LAUNCHER_EXPANDED_HEIGHT - LAUNCHER_HEIGHT).max(1.0);
        let expansion_progress =
            ((self.window_height - LAUNCHER_HEIGHT) / expansion_range).clamp(0.0, 1.0);
        let results_height = RESULTS_HEIGHT_NORMAL
            + (RESULTS_HEIGHT_EXPANDED - RESULTS_HEIGHT_NORMAL) * expansion_progress;

        let mut results = div()
            .id("results-list")
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .h(px(results_height))
            .track_scroll(&self.results_scroll)
            .overflow_y_scroll();

        if self.items.is_empty() {
            results = results.child(
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(palette.muted_text)
                    .text_sm()
                    .child("Clipboard is empty. Copy text/code/commands to get started."),
            );
        } else {
            for (ix, item) in self.items.iter().enumerate() {
                let is_selected = ix == self.selected_index;
                let item_created = format_timestamp(&item.created_at);
                let detected_language = detect_language(item.item_type, &item.content);
                let mut item_tags =
                    visible_tag_chips(item.item_type, detected_language, &item.tags);
                if !item.description.trim().is_empty() {
                    item_tags.insert(0, "INFO".to_owned());
                }
                if !item.parameters.is_empty() {
                    item_tags.insert(0, "PARAM".to_owned());
                    for parameter in item.parameters.iter().take(2) {
                        item_tags.push(format!("P:{}", parameter.name.to_ascii_uppercase()));
                    }
                }
                if item.item_type == ClipboardItemType::Password {
                    if let Some(seconds) = self.secret_seconds_left(item.id) {
                        item_tags.insert(0, format!("OPEN {seconds}s"));
                    } else {
                        item_tags.insert(0, "LOCKED".to_owned());
                    }
                }
                let is_masked_secret =
                    item.item_type == ClipboardItemType::Password && self.is_secret_masked(item.id);
                let is_selected_expanded =
                    selected_should_expand && is_selected && !is_masked_secret;
                let item_preview = if is_masked_secret {
                    masked_secret_preview(&item.content)
                } else if is_selected_expanded {
                    expanded_preview_content(&item.content)
                } else {
                    preview_content(&item.content)
                };
                let preview_language = if is_masked_secret {
                    None
                } else {
                    detected_language
                };
                let row_syntax_enabled = self.syntax_highlighting && is_selected;

                let mut row = div()
                    .id(("result", item.id as u64))
                    .w_full()
                    .p_1()
                    .rounded_lg()
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
                    let mut tags_row = div().flex().items_center().gap_1();
                    for tag in item_tags {
                        tags_row = tags_row.child(
                            div()
                                .text_xs()
                                .text_color(tag_chip_color(&tag, palette.dark))
                                .bg(scale_alpha(palette.row_hover_bg, 0.95))
                                .border_1()
                                .border_color(scale_alpha(palette.window_border, 0.85))
                                .rounded_md()
                                .px_1()
                                .child(tag),
                        );
                    }
                    top_row = top_row.child(tags_row);
                }
                top_row = top_row.child(
                    div()
                        .text_xs()
                        .text_color(palette.row_meta_text)
                        .child(item_created),
                );
                row = row.child(top_row);

                let mut preview_block = div()
                    .w_full()
                    .mt_1()
                    .text_sm()
                    .text_color(palette.row_text)
                    .whitespace_normal();
                if !is_selected_expanded {
                    preview_block = preview_block.line_clamp(4);
                }
                row = row.child(preview_block.child(syntax_styled_text(
                    &item_preview,
                    preview_language,
                    row_syntax_enabled,
                    palette.dark,
                )));

                if is_selected && !item.description.trim().is_empty() {
                    row = row.child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_xs()
                            .text_color(palette.row_meta_text)
                            .line_clamp(2)
                            .child(format!("ⓘ {}", item.description.trim())),
                    );
                }

                if is_selected {
                    row = row.border_1().border_color(palette.selected_border);
                }

                results = results.child(row);
            }
        }

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
            .child(if query_is_selected {
                div().w_full().child(
                    div()
                        .px_1()
                        .rounded_md()
                        .bg(scale_alpha(
                            palette.selected_bg,
                            if palette.dark { 0.95 } else { 0.75 },
                        ))
                        .text_lg()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(palette.row_text)
                        .child(query_display),
                )
            } else {
                div()
                    .w_full()
                    .text_lg()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(query_color)
                    .child(query_display)
            });

        if let Some(item_id) = self.info_editor_target_id {
            let info_display = if self.info_editor_input.is_empty() {
                "e.g. Removes stale Docker container and network".to_owned()
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
                        if palette.dark { 0.95 } else { 0.9 },
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
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(palette.muted_text)
                                    .child("Enter save • Esc cancel"),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_sm()
                            .text_color(info_color)
                            .child(info_display),
                    )
                    .child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child(
                                "Optional info for this snippet • ⌘V paste • Enter on empty clears",
                            ),
                    ),
            );
        }

        if let Some(item_id) = self.tag_editor_target_id {
            let input_display = if self.tag_editor_input.is_empty() {
                if self.tag_editor_mode == TagEditorMode::Add {
                    "e.g. DEVOPS,PROD,DOCKER".to_owned()
                } else {
                    "e.g. PROD,OLDTAG".to_owned()
                }
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
                        if palette.dark { 0.95 } else { 0.9 },
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
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(palette.muted_text)
                                    .child("Enter save • Esc cancel"),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_sm()
                            .text_color(input_color)
                            .child(input_display),
                    )
                    .child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child("Comma separated tags • ⌘V paste • case-insensitive"),
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
                let mut token_picker = div().w_full().mt_1().flex().flex_row().flex_wrap().gap_1();
                for (range_ix, range) in parameter_clickable_ranges(&item_content)
                    .into_iter()
                    .take(120)
                    .enumerate()
                {
                    let Some(token) = item_content.get(range.clone()) else {
                        continue;
                    };
                    if token.is_empty() {
                        continue;
                    }
                    let token = token.to_owned();
                    let is_selected = self
                        .parameter_editor_selected_targets
                        .iter()
                        .any(|existing| existing == &token);
                    let chip_bg = if is_selected {
                        scale_alpha(palette.selected_bg, if palette.dark { 0.9 } else { 0.6 })
                    } else {
                        scale_alpha(palette.row_hover_bg, 0.92)
                    };
                    let chip_border = if is_selected {
                        scale_alpha(palette.selected_border, 0.95)
                    } else {
                        scale_alpha(palette.window_border, 0.85)
                    };

                    token_picker = token_picker.child(
                        div()
                            .id(("parameter-token", range_ix as u64))
                            .text_xs()
                            .text_color(if is_selected {
                                if palette.dark {
                                    rgb(0xbfdbfe)
                                } else {
                                    rgb(0x1d4ed8)
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

                content = content.child(
                    div()
                        .w_full()
                        .p_2()
                        .bg(scale_alpha(
                            palette.row_hover_bg,
                            if palette.dark { 0.95 } else { 0.9 },
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
                                        .child(format!("Parametrize Snippet • Snippet #{item_id}")),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(palette.muted_text)
                                        .child("Select token buttons, then P/Enter"),
                                ),
                        )
                        .child(token_picker)
                        .child(
                            div()
                                .w_full()
                                .mt_1()
                                .text_xs()
                                .text_color(palette.muted_text)
                                .child(if self.parameter_editor_selected_targets.is_empty() {
                                    "Click a token • ⌘+click to multi-select • Tab/P/Enter next • Esc cancel"
                                } else {
                                    "⌘+click toggles additional tokens • Tab/P/Enter next • Esc cancel"
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
                        let is_focus = ix == self.parameter_editor_name_focus_index;
                        let value_display = if value.is_empty() {
                            "e.g. reg_id".to_owned()
                        } else {
                            value
                        };
                        let value_color = if value_display == "e.g. reg_id" {
                            palette.query_placeholder
                        } else {
                            palette.query_active
                        };

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
                                        if palette.dark { 0.92 } else { 0.88 },
                                    )
                                })
                                .border_1()
                                .border_color(if is_focus {
                                    palette.selected_border
                                } else {
                                    scale_alpha(palette.window_border, 0.88)
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
                            if palette.dark { 0.95 } else { 0.9 },
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
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(palette.muted_text)
                                        .child("Enter save • Esc cancel"),
                                ),
                        )
                        .child(name_rows)
                        .child(
                            div()
                                .w_full()
                                .mt_1()
                                .text_xs()
                                .text_color(palette.muted_text)
                                .child("Stage 2/2 • Tab/↑↓ switch field • Use letters, numbers, underscores • ⌘V paste"),
                        ),
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
                let is_focus = ix == self.parameter_fill_focus_index;
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
                                if palette.dark { 0.92 } else { 0.88 },
                            )
                        })
                        .border_1()
                        .border_color(if is_focus {
                            palette.selected_border
                        } else {
                            scale_alpha(palette.window_border, 0.88)
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
                        if palette.dark { 0.95 } else { 0.9 },
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
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(palette.muted_text)
                                    .child("Enter copy • Esc cancel"),
                            ),
                    )
                    .child(fill_rows)
                    .child(
                        div()
                            .w_full()
                            .mt_1()
                            .text_xs()
                            .text_color(palette.muted_text)
                            .child("Tab/↑↓ switch field • ⌘V paste • leave all fields empty to copy original"),
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
                    scale_alpha(palette.row_hover_bg, if palette.dark { 0.95 } else { 0.88 });
                let button_border = scale_alpha(palette.window_border, 0.9);
                let button_hover =
                    scale_alpha(palette.selected_bg, if palette.dark { 0.95 } else { 0.9 });
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
                        if palette.dark { 0.95 } else { 0.9 },
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
                            if palette.dark { 0.95 } else { 0.9 },
                        ))
                        .border_1()
                        .border_color(scale_alpha(palette.window_border, 0.9))
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
