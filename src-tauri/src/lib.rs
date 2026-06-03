#![cfg_attr(not(feature = "app"), allow(dead_code))]

mod audio;
#[cfg(feature = "app")]
mod commands;
mod config;
#[cfg(all(feature = "app", windows))]
mod single_instance;

#[cfg(feature = "app")]
pub fn run() {
    #[cfg(windows)]
    if !single_instance::claim_or_focus_existing() {
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(commands::PipeMicState::new())
        .setup(commands::setup_app)
        .on_window_event(commands::handle_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::list_capture_devices,
            commands::list_render_devices,
            commands::list_sessions,
            commands::load_config,
            commands::save_config,
            commands::apply_app_settings,
            commands::start_routing,
            commands::stop_routing,
            commands::get_status,
            commands::update_controls,
            commands::open_source_url
        ])
        .run(tauri::generate_context!())
        .expect("failed to run PipeMic");
}

#[cfg(not(feature = "app"))]
pub fn run() {
    panic!("PipeMic was built without the app feature");
}
