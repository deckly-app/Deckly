//! Windows: `ShellExecuteEx` passes the show command and a monitor to the
//! started program. Many programs ignore the monitor and some the show
//! command, so once the program shows its first window Deckly places that
//! window itself. Launchers start the real program as a child process, so
//! windows of the started process's children count too.
//!
//! What cannot be placed: a program that was already running, or one that
//! hands the request to a process it did not start (a browser opening a
//! link, Store apps). Windows of programs running as administrator cannot be
//! moved by Deckly unless Deckly runs as administrator too.

use super::super::command::split_words;
use super::{Monitor, Placement, Support, WindowState};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    DISPLAY_DEVICEW, EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR,
    MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{GetProcessId, WaitForInputIdle};
use windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_HMONITOR, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
    SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EDD_GET_DEVICE_INTERFACE_NAME, EnumWindows, GW_OWNER, GetWindow, GetWindowPlacement,
    GetWindowThreadProcessId, IsWindowVisible, SHOW_WINDOW_CMD, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED,
    SW_SHOWMINNOACTIVE, SW_SHOWNORMAL, SetWindowPlacement, ShowWindow, WINDOWPLACEMENT,
};
use windows::core::{BOOL, HSTRING, PCWSTR, w};

/// How long Deckly looks for the started program's first window.
const WAIT_FOR_WINDOW: Duration = Duration::from_secs(15);
const LOOK_AGAIN: Duration = Duration::from_millis(150);
const MAX_MONITORS: usize = 32;
/// The primary monitor's flag in `MONITORINFO`.
const PRIMARY: u32 = 1;

pub fn support() -> Support {
    Support {
        monitor: true,
        states: vec![
            WindowState::Normal,
            WindowState::Maximized,
            WindowState::Minimized,
        ],
        layout: false,
        note: Some(
            "The window settings apply to a new window of the started program: a \
             program that is already running keeps its window where it is."
                .into(),
        ),
    }
}

struct Screen {
    handle: HMONITOR,
    monitor: Monitor,
    whole: RECT,
    work: RECT,
    primary: bool,
}

fn wide(text: &[u16]) -> String {
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}

fn screens() -> Vec<Screen> {
    unsafe extern "system" fn add(handle: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `&mut Vec<HMONITOR>` passed to
        // EnumDisplayMonitors below, alive and unaliased for its whole call.
        let list = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
        list.push(handle);
        BOOL::from(list.len() < MAX_MONITORS)
    }
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only pushes to `handles`, which outlives the call.
    let _ =
        unsafe { EnumDisplayMonitors(None, None, Some(add), LPARAM(&raw mut handles as isize)) };
    let mut list = Vec::new();
    for (number, handle) in handles.into_iter().enumerate() {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        // SAFETY: `info` is a MONITORINFOEXW with its size set, as the call
        // allows for a MONITORINFO pointer.
        if !unsafe { GetMonitorInfoW(handle, &raw mut info as *mut MONITORINFO) }.as_bool() {
            continue;
        }
        let device = wide(&info.szDevice);
        let (id, name) = monitor_device(&device);
        let whole = info.monitorInfo.rcMonitor;
        let primary = info.monitorInfo.dwFlags & PRIMARY != 0;
        let size = format!("{}×{}", whole.right - whole.left, whole.bottom - whole.top);
        let mut label = match name {
            Some(name) => format!("{name} ({size})"),
            None => format!("Monitor {} ({size})", number + 1),
        };
        if primary {
            label.push_str(", main");
        }
        list.push(Screen {
            handle,
            monitor: Monitor {
                id: id.unwrap_or_else(|| device.clone()),
                label,
                port: device,
            },
            whole,
            work: info.monitorInfo.rcWork,
            primary,
        });
    }
    list
}

/// The screen's monitor: its device path, which names it on whatever
/// adapter output it is plugged into until Windows re-detects it, and its
/// name when the driver gives a better one than "Generic PnP Monitor".
fn monitor_device(adapter: &str) -> (Option<String>, Option<String>) {
    let mut device = DISPLAY_DEVICEW {
        cb: size_of::<DISPLAY_DEVICEW>() as u32,
        ..Default::default()
    };
    // SAFETY: the name is a NUL-terminated wide string that outlives the
    // call, and `device` has its size set.
    let found = unsafe {
        EnumDisplayDevicesW(
            &HSTRING::from(adapter),
            0,
            &raw mut device,
            EDD_GET_DEVICE_INTERFACE_NAME,
        )
    };
    if !found.as_bool() {
        return (None, None);
    }
    let id = Some(wide(&device.DeviceID)).filter(|s| !s.is_empty());
    let name = Some(wide(&device.DeviceString))
        .filter(|s| !s.is_empty() && !s.contains("Generic") && !s.contains("PnP"));
    (id, name)
}

pub fn monitors() -> Result<Vec<Monitor>, String> {
    let list: Vec<Monitor> = screens().into_iter().map(|s| s.monitor).collect();
    if list.is_empty() {
        Err("Windows reports no monitors".into())
    } else {
        Ok(list)
    }
}

fn show_command(state: WindowState) -> SHOW_WINDOW_CMD {
    match state {
        WindowState::Maximized => SW_SHOWMAXIMIZED,
        // Started in the background, so it does not take the focus.
        WindowState::Minimized => SW_SHOWMINNOACTIVE,
        _ => SW_SHOWNORMAL,
    }
}

/// A process handle that is closed once.
struct Process(HANDLE);

// SAFETY: a process handle is a kernel handle, usable from any thread.
unsafe impl Send for Process {}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle came from ShellExecuteEx and is closed only here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Starts `file` with `parameters`. `None` when Windows cannot; the process
/// when Windows started one Deckly can follow.
fn execute(
    file: &str,
    parameters: &str,
    show: SHOW_WINDOW_CMD,
    monitor: Option<HMONITOR>,
) -> Option<Option<Process>> {
    let file = HSTRING::from(file);
    let parameters = HSTRING::from(parameters);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: w!("open"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: show.0,
        ..Default::default()
    };
    if let Some(monitor) = monitor {
        info.fMask |= SEE_MASK_HMONITOR;
        info.Anonymous.hMonitor = HANDLE(monitor.0);
    }
    // SAFETY: `info` has its size set and its strings outlive the call.
    unsafe { ShellExecuteExW(&raw mut info) }.ok()?;
    Some((!info.hProcess.is_invalid()).then(|| Process(info.hProcess)))
}

/// Starts `target` as [`super::super::open`] does and places its window.
/// Returns notes on what it could not apply.
pub fn open(target: &str, placement: &Placement) -> Result<Vec<String>, String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("nothing to open".into());
    }
    let mut notes = Vec::new();
    let screens = screens();
    let mut screen = None;
    if !placement.monitor.is_empty() {
        screen = screens
            .iter()
            .position(|s| s.monitor.id == placement.monitor);
        if screen.is_none() {
            notes.push(super::missing_monitor(placement));
        }
    }
    let show = show_command(placement.state);
    let handle = screen.map(|i| screens[i].handle);
    let started = match execute(target, "", show, handle) {
        Some(started) => started,
        None => {
            let words =
                split_words(target).ok_or_else(|| format!("unclosed quote in \"{target}\""))?;
            let (program, args) = words
                .split_first()
                .ok_or_else(|| format!("cannot open \"{target}\""))?;
            let args: Vec<String> = args
                .iter()
                .map(|a| super::super::windows::quoted(a))
                .collect();
            execute(program, &args.join(" "), show, handle)
                .ok_or_else(|| format!("cannot open \"{target}\""))?
        }
    };
    if let Some(process) = started {
        let area = screen.map(|i| Area::of(&screens[i], &screens));
        let state = placement.state;
        let follow = move || place_first_window(process, area, state);
        if std::thread::Builder::new()
            .name("deckly-place-window".into())
            .stack_size(256 * 1024)
            .spawn(follow)
            .is_err()
        {
            notes.push("the window could not be moved".into());
        }
    }
    Ok(notes)
}

/// Where a window goes: a monitor's work area, in the "workspace"
/// coordinates of `WINDOWPLACEMENT`, which start at the main monitor's work
/// area instead of its corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Area {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Area {
    fn of(screen: &Screen, all: &[Screen]) -> Self {
        let (dx, dy) = all.iter().find(|s| s.primary).map_or((0, 0), |p| {
            (p.work.left - p.whole.left, p.work.top - p.whole.top)
        });
        Area {
            left: screen.work.left - dx,
            top: screen.work.top - dy,
            right: screen.work.right - dx,
            bottom: screen.work.bottom - dy,
        }
    }

    /// `window`'s size, at most the area's, centred in it.
    fn fit(self, window: RECT) -> RECT {
        let width = (window.right - window.left).clamp(1, (self.right - self.left).max(1));
        let height = (window.bottom - window.top).clamp(1, (self.bottom - self.top).max(1));
        let left = self.left + (self.right - self.left - width) / 2;
        let top = self.top + (self.bottom - self.top - height) / 2;
        RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }
}

fn place_first_window(process: Process, area: Option<Area>, state: WindowState) {
    // SAFETY: the handle is a live process handle owned by `process`.
    let pid = unsafe { GetProcessId(process.0) };
    // Lets a program finish starting before Deckly looks for its window.
    // SAFETY: as above; a console program returns at once with an error.
    let _ = unsafe { WaitForInputIdle(process.0, 5_000) };
    let deadline = Instant::now() + WAIT_FOR_WINDOW;
    while Instant::now() < deadline {
        if pid != 0
            && let Some(window) = first_window(&family(pid))
        {
            if let Err(e) = place(window, area, state) {
                eprintln!("deckly: cannot place the opened window: {e}");
            }
            return;
        }
        std::thread::sleep(LOOK_AGAIN);
    }
}

/// `pid` and the processes it started, and theirs.
fn family(pid: u32) -> HashSet<u32> {
    let mut family = HashSet::from([pid]);
    // SAFETY: a snapshot of all processes; the handle is closed below.
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return family;
    };
    let mut parents = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `entry` has its size set; the snapshot handle is valid.
    let mut more = unsafe { Process32FirstW(snapshot, &raw mut entry) }.is_ok();
    while more && parents.len() < 65_536 {
        parents.push((entry.th32ProcessID, entry.th32ParentProcessID));
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &raw mut entry) }.is_ok();
    }
    // SAFETY: the snapshot is closed once, here.
    let _ = unsafe { CloseHandle(snapshot) };
    // Children by their parent, a few levels deep.
    for _ in 0..4 {
        let before = family.len();
        for &(child, parent) in &parents {
            if family.contains(&parent) && child != 0 {
                family.insert(child);
            }
        }
        if family.len() == before {
            break;
        }
    }
    family
}

/// A visible top-level window, not owned by another, of one of `pids`.
fn first_window(pids: &HashSet<u32>) -> Option<HWND> {
    struct Search<'a> {
        pids: &'a HashSet<u32>,
        found: Option<HWND>,
    }
    unsafe extern "system" fn check(window: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `&mut Search` passed to EnumWindows below,
        // alive and unaliased for its whole call.
        let search = unsafe { &mut *(data.0 as *mut Search) };
        let mut pid = 0;
        // SAFETY: `window` comes from EnumWindows; `pid` outlives the call.
        unsafe { GetWindowThreadProcessId(window, Some(&raw mut pid)) };
        // SAFETY: plain queries on a window handle from EnumWindows.
        let candidate = search.pids.contains(&pid)
            && unsafe { IsWindowVisible(window) }.as_bool()
            && unsafe { GetWindow(window, GW_OWNER) }.is_err();
        if candidate {
            search.found = Some(window);
        }
        BOOL::from(!candidate)
    }
    let mut search = Search { pids, found: None };
    // SAFETY: the callback only touches `search`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(check), LPARAM(&raw mut search as isize)) };
    search.found
}

fn place(window: HWND, area: Option<Area>, state: WindowState) -> Result<(), String> {
    let mut placement = WINDOWPLACEMENT {
        length: size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    // SAFETY: `placement` has its length set; `window` is a window handle.
    unsafe { GetWindowPlacement(window, &raw mut placement) }.map_err(|e| e.to_string())?;
    if let Some(area) = area {
        placement.rcNormalPosition = area.fit(placement.rcNormalPosition);
    }
    placement.showCmd = match state {
        WindowState::Default => placement.showCmd,
        WindowState::Minimized => SW_SHOWMINIMIZED.0 as u32,
        other => show_command(other).0 as u32,
    };
    if area.is_some() {
        // SAFETY: as above.
        unsafe { SetWindowPlacement(window, &raw const placement) }.map_err(|e| e.to_string())?;
    } else if state != WindowState::Default {
        // SAFETY: `window` is a window handle.
        let _ = unsafe { ShowWindow(window, show_command(state)) };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_keeps_its_size_and_is_centred_in_the_monitor() {
        let area = Area {
            left: 1920,
            top: 0,
            right: 3840,
            bottom: 1040,
        };
        let window = RECT {
            left: 10,
            top: 20,
            right: 810,
            bottom: 620,
        };
        let placed = area.fit(window);
        assert_eq!((placed.left, placed.top), (2480, 220));
        assert_eq!(
            (placed.right - placed.left, placed.bottom - placed.top),
            (800, 600)
        );
        let huge = RECT {
            left: 0,
            top: 0,
            right: 5000,
            bottom: 5000,
        };
        assert_eq!(
            area.fit(huge),
            RECT {
                left: 1920,
                top: 0,
                right: 3840,
                bottom: 1040
            }
        );
    }

    #[test]
    fn this_computer_lists_its_monitors() {
        // A build machine may have no monitor at all.
        if let Ok(list) = monitors() {
            assert!(list.iter().all(|m| !m.id.is_empty() && !m.label.is_empty()));
        }
    }
}
