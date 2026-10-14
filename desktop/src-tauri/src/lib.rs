mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::engine_info,
            commands::self_test,
            commands::apply_document
        ])
        .run(tauri::generate_context!())
        .expect("error while running prosemirror-rs");
}
