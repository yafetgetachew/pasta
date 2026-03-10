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
        let tag_editor_open = self.tag_editor_target_id.is_some();
        let transform_menu_open = self.transform_menu_open;
        let selection_stable = Instant::now().duration_since(self.selection_changed_at)
            >= Duration::from_millis(SELECTION_EXPAND_DWELL_MS);
        let selected_should_expand = selection_stable
            && !tag_editor_open
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
                if !tag_editor_open && !transform_menu_open {
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
                            .child("OPTION+SPACE"),
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
                                .child("Search • /tag tag-only • Enter copy • Tab transforms • ⌘R reveal+copy secret"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(palette.muted_text)
                                .child("⌘J/⌘K/⌘L/⌘; navigate • ⌘⇧S mark secret • ⌘T add tags • ⌘⇧T remove tags • ⌘D delete • Esc close • ⌘Q close • ⌘H hide help"),
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
