mod commands;
mod git;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(commands::RepoRegistry::default())
        .invoke_handler(tauri::generate_handler![
            commands::open_repo,
            commands::close_repo,
            commands::list_refs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
