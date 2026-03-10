#[cfg(target_os = "macos")]
use crate::*;

pub(crate) struct SearchRequest {
    pub(crate) request_id: u64,
    pub(crate) query: String,
}

#[cfg(target_os = "macos")]
pub(crate) struct SearchResponse {
    pub(crate) request_id: u64,
    pub(crate) items: Vec<ClipboardRecord>,
}

#[cfg(target_os = "macos")]
pub(crate) fn start_search_worker(
    storage: Arc<ClipboardStorage>,
) -> (mpsc::Sender<SearchRequest>, mpsc::Receiver<SearchResponse>) {
    let (request_tx, request_rx) = mpsc::channel::<SearchRequest>();
    let (result_tx, result_rx) = mpsc::channel::<SearchResponse>();

    let spawn_result = std::thread::Builder::new()
        .name("pasta-search-worker".to_owned())
        .spawn(move || {
            while let Ok(mut request) = request_rx.recv() {
                while let Ok(newer) = request_rx.try_recv() {
                    request = newer;
                }

                let items = storage
                    .search_items(&request.query, 48)
                    .unwrap_or_else(|err| {
                        eprintln!("warning: search worker failed to query clipboard items: {err}");
                        Vec::new()
                    });

                if result_tx
                    .send(SearchResponse {
                        request_id: request.request_id,
                        items,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
    if let Err(err) = spawn_result {
        eprintln!("warning: failed to start search worker thread: {err}");
    }

    (request_tx, result_rx)
}

#[cfg(target_os = "macos")]
pub(crate) struct LauncherView {
    pub(crate) storage: Arc<ClipboardStorage>,
    pub(crate) font_family: SharedString,
    pub(crate) surface_alpha: f32,
    pub(crate) syntax_highlighting: bool,
    pub(crate) results_scroll: ScrollHandle,
    pub(crate) search_request_tx: mpsc::Sender<SearchRequest>,
    pub(crate) search_result_rx: mpsc::Receiver<SearchResponse>,
    pub(crate) next_search_request_id: u64,
    pub(crate) latest_search_request_id: u64,
    pub(crate) query: String,
    pub(crate) query_refresh_due_at: Option<Instant>,
    pub(crate) query_select_all: bool,
    pub(crate) items: Vec<ClipboardRecord>,
    pub(crate) selected_index: usize,
    pub(crate) selection_changed_at: Instant,
    pub(crate) transition_alpha: f32,
    pub(crate) transition_from: f32,
    pub(crate) transition_target: f32,
    pub(crate) transition_started_at: Instant,
    pub(crate) transition_duration: Duration,
    pub(crate) pending_exit: Option<LauncherExitIntent>,
    pub(crate) revealed_secret_id: Option<i64>,
    pub(crate) reveal_until: Option<Instant>,
    pub(crate) last_reveal_second_bucket: Option<u64>,
    pub(crate) tag_editor_target_id: Option<i64>,
    pub(crate) tag_editor_input: String,
    pub(crate) tag_editor_mode: TagEditorMode,
    pub(crate) parameter_editor_target_id: Option<i64>,
    pub(crate) parameter_editor_name_input: String,
    pub(crate) parameter_editor_stage: ParameterEditorStage,
    pub(crate) parameter_editor_selected_targets: Vec<String>,
    pub(crate) parameter_editor_name_inputs: Vec<String>,
    pub(crate) parameter_editor_name_focus_index: usize,
    pub(crate) parameter_fill_target_id: Option<i64>,
    pub(crate) parameter_fill_input: String,
    pub(crate) parameter_fill_values: Vec<String>,
    pub(crate) parameter_fill_focus_index: usize,
    pub(crate) transform_menu_open: bool,
    pub(crate) window_height: f32,
    pub(crate) applied_window_height: f32,
    pub(crate) window_height_from: f32,
    pub(crate) window_height_target: f32,
    pub(crate) window_height_started_at: Instant,
    pub(crate) window_height_duration: Duration,
    pub(crate) blur_close_armed: bool,
    pub(crate) suppress_auto_hide: bool,
    pub(crate) suppress_auto_hide_until: Option<Instant>,
    pub(crate) show_command_help: bool,
    pub(crate) last_window_appearance: Option<WindowAppearance>,
}
