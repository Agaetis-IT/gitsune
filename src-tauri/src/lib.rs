mod commands;
mod git;
mod layout;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::RepoRegistry::default())
        .invoke_handler(tauri::generate_handler![
            commands::open_repo,
            commands::close_repo,
            commands::list_refs,
            commands::get_graph_meta,
            commands::get_graph_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
