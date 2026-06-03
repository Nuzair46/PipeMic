use std::sync::OnceLock;

use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE},
        System::Threading::CreateMutexW,
        UI::WindowsAndMessaging::{
            FindWindowW, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
        },
    },
    core::PCWSTR,
};

const MUTEX_NAME: &str = "Local\\com.pipemic.desktop.single-instance";
const WINDOW_TITLE: &str = "PipeMic";

static INSTANCE: OnceLock<SingleInstanceGuard> = OnceLock::new();

pub fn claim_or_focus_existing() -> bool {
    match create_instance_guard() {
        Ok(Some(guard)) => {
            let _ = INSTANCE.set(guard);
            true
        }
        Ok(None) => {
            focus_existing_window();
            false
        }
        Err(error) => {
            eprintln!("PipeMic single-instance warning: {error}");
            true
        }
    }
}

fn create_instance_guard() -> windows_core::Result<Option<SingleInstanceGuard>> {
    let name = wide_string(MUTEX_NAME);
    let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr()))? };
    let already_running = unsafe { GetLastError() == ERROR_ALREADY_EXISTS };
    if already_running {
        let _ = unsafe { CloseHandle(handle) };
        Ok(None)
    } else {
        Ok(Some(SingleInstanceGuard(handle)))
    }
}

fn focus_existing_window() {
    let title = wide_string(WINDOW_TITLE);
    if let Ok(hwnd) = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) } {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

fn wide_string(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

struct SingleInstanceGuard(HANDLE);

unsafe impl Send for SingleInstanceGuard {}
unsafe impl Sync for SingleInstanceGuard {}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}
