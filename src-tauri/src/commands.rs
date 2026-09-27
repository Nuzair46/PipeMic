use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{
    Manager, State,
    menu::MenuBuilder,
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
};

use crate::{
    audio::{
        self,
        discovery::Discovery,
        engine::AudioEngine,
        types::{AudioDevice, AudioSession, RouteStatus},
    },
    config::{self, AppConfig, AppSettings, ControlUpdate},
    service::RoutingService,
};

const SOURCE_URL: &str = "https://github.com/nuzair46/pipemic";
const RELEASES_URL: &str = "https://github.com/nuzair46/pipemic/releases";
const TRAY_SHOW_ID: &str = "show";
const TRAY_QUIT_ID: &str = "quit";

pub struct PipeMicState {
    service: Arc<Mutex<RoutingService<AudioEngine>>>,
    discovery: Arc<Discovery>,
    minimize_to_tray: Arc<AtomicBool>,
    quit_requested: AtomicBool,
}

impl PipeMicState {
    pub fn new() -> Self {
        let discovery = Arc::new(Discovery::new());
        let service = RoutingService::new(AudioEngine::new(Arc::clone(&discovery)));
        let minimize_to_tray = Arc::new(AtomicBool::new(service.minimize_to_tray()));
        Self {
            service: Arc::new(Mutex::new(service)),
            discovery,
            minimize_to_tray,
            quit_requested: AtomicBool::new(false),
        }
    }

    fn config_snapshot(&self) -> Result<AppConfig, String> {
        self.service.lock().config()
    }

    fn should_hide_on_close(&self) -> bool {
        self.minimize_to_tray.load(Ordering::Acquire)
            && !self.quit_requested.load(Ordering::Acquire)
    }
}

fn request_quit(app: &tauri::AppHandle) {
    let state = app.state::<PipeMicState>();
    if state.quit_requested.swap(true, Ordering::AcqRel) {
        return;
    }
    let service = Arc::clone(&state.service);
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = service.lock().shutdown() {
            eprintln!("Could not flush settings on exit: {error}");
        }
        app.exit(0);
    });
}

async fn with_service<T: Send + 'static>(
    state: State<'_, PipeMicState>,
    action: impl FnOnce(&mut RoutingService<AudioEngine>) -> Result<T, String> + Send + 'static,
) -> CommandResult<T> {
    let service = Arc::clone(&state.service);
    let minimize_to_tray = Arc::clone(&state.minimize_to_tray);
    tauri::async_runtime::spawn_blocking(move || {
        let mut service = service.lock();
        let result = action(&mut service);
        minimize_to_tray.store(service.minimize_to_tray(), Ordering::Release);
        result
    })
    .await
    .map_err(|error| CommandError {
        message: error.to_string(),
    })?
    .map_err(|message| CommandError { message })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    message: String,
}

impl From<audio::AudioError> for CommandError {
    fn from(error: audio::AudioError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl From<config::ConfigError> for CommandError {
    fn from(error: config::ConfigError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl From<StartupSettingsError> for CommandError {
    fn from(error: StartupSettingsError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

type CommandResult<T> = Result<T, CommandError>;

#[derive(Debug, thiserror::Error)]
enum StartupSettingsError {
    #[error("could not resolve the PipeMic executable: {0}")]
    CurrentExe(#[from] std::io::Error),
    #[error("could not update Windows startup settings: {0}")]
    Registry(String),
}

pub fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    setup_tray(app)?;
    if let Ok(config) = app.state::<PipeMicState>().config_snapshot() {
        if let Err(error) = apply_startup_settings(&config) {
            eprintln!("PipeMic startup setting warning: {error}");
        }
        if should_start_minimized(&config) {
            hide_main_window(app.handle());
        }
    }

    Ok(())
}

pub fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        let state = window.state::<PipeMicState>();
        if state.should_hide_on_close() {
            api.prevent_close();
            let _ = window.hide();
        } else {
            api.prevent_close();
            request_quit(window.app_handle());
        }
    }
}

async fn discovery(state: State<'_, PipeMicState>) -> CommandResult<audio::discovery::Snapshot> {
    let discovery = Arc::clone(&state.discovery);
    tauri::async_runtime::spawn_blocking(move || discovery.initial_snapshot())
        .await
        .map_err(|error| CommandError {
            message: error.to_string(),
        })
}

#[tauri::command]
pub async fn list_capture_devices(
    state: State<'_, PipeMicState>,
) -> CommandResult<Vec<AudioDevice>> {
    discovery(state)
        .await?
        .capture
        .map_err(|message| CommandError { message })
}
#[tauri::command]
pub async fn list_render_devices(
    state: State<'_, PipeMicState>,
) -> CommandResult<Vec<AudioDevice>> {
    discovery(state)
        .await?
        .render
        .map_err(|message| CommandError { message })
}
#[tauri::command]
pub async fn list_sessions(state: State<'_, PipeMicState>) -> CommandResult<Vec<AudioSession>> {
    discovery(state)
        .await?
        .sessions
        .map_err(|message| CommandError { message })
}
#[tauri::command]
pub async fn load_config(state: State<'_, PipeMicState>) -> CommandResult<AppConfig> {
    with_service(state, move |service| service.config()).await
}
#[tauri::command]
pub async fn save_config(
    config: AppConfig,
    state: State<'_, PipeMicState>,
) -> CommandResult<AppConfig> {
    with_service(state, move |service| service.save(config)).await
}
#[tauri::command]
pub async fn apply_app_settings(
    config: AppSettings,
    state: State<'_, PipeMicState>,
) -> CommandResult<AppConfig> {
    with_service(state, move |service| {
        let mut next = service.config()?;
        next.apply_settings(config.clone());
        apply_startup_settings(&next).map_err(|error| error.to_string())?;
        service.settings(config)
    })
    .await
}
#[tauri::command]
pub async fn start_routing(
    config: AppConfig,
    state: State<'_, PipeMicState>,
) -> CommandResult<RouteStatus> {
    with_service(state, move |service| service.start(config)).await
}
#[tauri::command]
pub async fn stop_routing(state: State<'_, PipeMicState>) -> CommandResult<RouteStatus> {
    with_service(state, move |service| Ok(service.stop())).await
}
#[tauri::command]
pub async fn get_status(state: State<'_, PipeMicState>) -> CommandResult<RouteStatus> {
    with_service(state, move |service| Ok(service.status())).await
}
#[tauri::command]
pub async fn update_controls(
    controls: ControlUpdate,
    state: State<'_, PipeMicState>,
) -> CommandResult<RouteStatus> {
    with_service(state, move |service| service.controls(controls)).await
}

#[tauri::command]
pub fn open_source_url() -> CommandResult<()> {
    open_url(SOURCE_URL).map_err(|message| CommandError { message })
}

#[tauri::command]
pub fn open_releases_url(url: Option<String>) -> CommandResult<()> {
    let target = url
        .as_deref()
        .filter(|value| value.starts_with(RELEASES_URL))
        .unwrap_or(RELEASES_URL);
    open_url(target).map_err(|message| CommandError { message })
}

#[cfg(windows)]
fn open_url(url: &str) -> Result<(), String> {
    use windows::{
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        core::{HSTRING, PCWSTR, w},
    };

    let url = HSTRING::from(url);
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(url.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let result_code = result.0 as isize;
    if result_code <= 32 {
        return Err(format!(
            "Windows could not open the source URL, ShellExecute returned {}",
            result_code
        ));
    }

    Ok(())
}

#[cfg(not(windows))]
fn open_url(url: &str) -> Result<(), String> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open {url}: {error}"))
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text(TRAY_SHOW_ID, "Show")
        .separator()
        .text(TRAY_QUIT_ID, "Quit")
        .build()?;

    let mut tray = TrayIconBuilder::new()
        .tooltip("PipeMic")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_SHOW_ID => show_main_window(app),
            TRAY_QUIT_ID => request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    Ok(())
}

fn show_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn hide_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn should_start_minimized(config: &AppConfig) -> bool {
    config.minimize_to_tray && std::env::args().any(|arg| arg == "--minimized")
}

#[cfg(windows)]
fn apply_startup_settings(config: &AppConfig) -> Result<(), StartupSettingsError> {
    use windows::{
        Win32::{
            Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
            System::Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
                RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
            },
        },
        core::PCWSTR,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "PipeMic";

    let run_key = wide_string(RUN_KEY);
    let value_name = wide_string(VALUE_NAME);
    let mut key = HKEY(std::ptr::null_mut());
    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(run_key.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    if result != ERROR_SUCCESS {
        return Err(StartupSettingsError::Registry(format!(
            "RegCreateKeyExW returned {}",
            result.0
        )));
    }

    let _guard = RegistryKey(key);
    if config.start_with_windows {
        let command = startup_command(config)?;
        let value = wide_string(&command);
        let bytes = wide_bytes(&value);
        let result =
            unsafe { RegSetValueExW(key, PCWSTR(value_name.as_ptr()), 0, REG_SZ, Some(bytes)) };
        if result != ERROR_SUCCESS {
            return Err(StartupSettingsError::Registry(format!(
                "RegSetValueExW returned {}",
                result.0
            )));
        }
    } else {
        let result = unsafe { RegDeleteValueW(key, PCWSTR(value_name.as_ptr())) };
        if result != ERROR_SUCCESS && result != ERROR_FILE_NOT_FOUND {
            return Err(StartupSettingsError::Registry(format!(
                "RegDeleteValueW returned {}",
                result.0
            )));
        }
    }

    Ok(())
}

#[cfg(windows)]
fn startup_command(config: &AppConfig) -> Result<String, StartupSettingsError> {
    let exe = std::env::current_exe()?;
    let minimized = if config.minimize_to_tray {
        " --minimized"
    } else {
        ""
    };
    Ok(format!("\"{}\"{minimized}", exe.display()))
}

#[cfg(windows)]
fn wide_string(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn wide_bytes(value: &[u16]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2) }
}

#[cfg(windows)]
struct RegistryKey(windows::Win32::System::Registry::HKEY);

#[cfg(windows)]
impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::System::Registry::RegCloseKey(self.0);
        }
    }
}

#[cfg(not(windows))]
fn apply_startup_settings(_config: &AppConfig) -> Result<(), StartupSettingsError> {
    Ok(())
}
