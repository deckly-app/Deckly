//! Windows: each visible top-level window of the application gets
//! `WM_CLOSE`, the message its close button sends. Hosted packaged apps are
//! known by the app id of their frame window. A program without such a window
//! (in the tray, or a script) has no message to take, so it is ended.

use super::{Closed, MAX_ITEMS};
use crate::os::active_window::windows::identify;
use deckly_core::AppRule;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    WM_CLOSE,
};
use windows::core::BOOL;

pub fn close(rule: &AppRule) -> Result<Closed, String> {
    let closed = close_windows(rule);
    if closed > 0 {
        return Ok(Closed::Windows(closed));
    }
    Ok(Closed::Processes(end_processes(rule)))
}

/// COM for reading a window's app id, for as long as the guard lives.
struct Com(bool);

impl Com {
    fn new() -> Self {
        // SAFETY: balanced by the drop below when it succeeds; a thread in
        // another apartment already refuses and is left as it is.
        Self(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: undoes the successful call in `new`, on the same thread.
            unsafe { CoUninitialize() };
        }
    }
}

fn close_windows(rule: &AppRule) -> usize {
    let _com = Com::new();
    let mut windows: Vec<HWND> = Vec::new();
    unsafe extern "system" fn add(window: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `&mut Vec` passed to EnumWindows below, alive
        // and unaliased for its whole call.
        let windows = unsafe { &mut *(data.0 as *mut Vec<HWND>) };
        // SAFETY: plain queries on a window handle from EnumWindows.
        let shown = unsafe { IsWindowVisible(window) }.as_bool()
            && unsafe { GetWindow(window, GW_OWNER) }.is_err();
        if shown {
            windows.push(window);
        }
        BOOL::from(windows.len() < MAX_ITEMS)
    }
    // SAFETY: the callback only touches `windows`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(add), LPARAM(&raw mut windows as isize)) };
    let own = std::process::id();
    let mut closed = 0;
    for window in windows {
        let mut pid = 0;
        // SAFETY: `pid` outlives the call; a stale window leaves it at 0.
        unsafe { GetWindowThreadProcessId(window, Some(&raw mut pid)) };
        if pid == own || !rule.matches(&identify(window)) {
            continue;
        }
        // SAFETY: posting carries no pointer; a window gone meanwhile fails.
        if unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok() {
            closed += 1;
        }
    }
    closed
}

/// Ends each process whose program the rule names. Returns how many ended.
fn end_processes(rule: &AppRule) -> usize {
    // SAFETY: a snapshot of all processes; the handle is closed below.
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return 0;
    };
    let mut matching = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let own = std::process::id();
    let mut seen = 0;
    // SAFETY: `entry` has its size set; the snapshot handle is valid.
    let mut more = unsafe { Process32FirstW(snapshot, &raw mut entry) }.is_ok();
    while more && seen < MAX_ITEMS {
        seen += 1;
        let len = entry
            .szExeFile
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(entry.szExeFile.get(..len).unwrap_or_default());
        if entry.th32ProcessID != own && entry.th32ProcessID != 0 && rule.matches(&[name]) {
            matching.push(entry.th32ProcessID);
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &raw mut entry) }.is_ok();
    }
    // SAFETY: the snapshot is closed once, here.
    let _ = unsafe { CloseHandle(snapshot) };
    let mut ended = 0;
    for pid in matching {
        // SAFETY: the handle is closed right after use; a process Deckly may
        // not end refuses to open.
        unsafe {
            if let Ok(process) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                if TerminateProcess(process, 1).is_ok() {
                    ended += 1;
                }
                let _ = CloseHandle(process);
            }
        }
    }
    ended
}
