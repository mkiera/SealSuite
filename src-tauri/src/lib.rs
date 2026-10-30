mod apps;
mod manage;
mod profile;
mod releases;
mod selfupdate;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            selfupdate::clear_updates_dir(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            manage::list_apps,
            manage::check_updates,
            manage::install_app,
            manage::open_app,
            manage::open_repo,
            selfupdate::self_update_build_info,
            selfupdate::self_update_get_settings,
            selfupdate::self_update_set_channel,
            selfupdate::self_update_set_auto_check,
            selfupdate::self_update_check,
            selfupdate::self_update_available,
            selfupdate::self_update_list_releases,
            selfupdate::self_update_list_alpha_builds,
            selfupdate::self_update_install_release,
            selfupdate::self_update_install_alpha,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
