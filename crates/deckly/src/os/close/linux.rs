//! Linux: on X11 the window manager closes a window when asked through the
//! root window (EWMH `_NET_CLOSE_WINDOW`), or the window itself is sent
//! `WM_DELETE_WINDOW`. On Wayland a client may ask the compositor to close
//! another's window only through the wlr foreign toplevel protocol, which
//! wlroots desktops (Hyprland, Sway, labwc and others) offer. GNOME and KDE do
//! not; there the program gets a termination signal, as a program without a
//! window does everywhere.

use super::{Closed, MAX_ITEMS, command_names};
use crate::os::active_window::linux::{class_names, gone_is_none, is_wayland, program};
use deckly_core::AppRule;
use std::error::Error;
use std::sync::OnceLock;

const NO_WINDOW_CLOSE: &str = "This desktop does not let other applications close its windows. \
     Deckly asks the program to quit instead, so it may not ask about unsaved work.";

pub fn close(rule: &AppRule) -> Result<Closed, String> {
    let windows = if is_wayland() {
        wayland::close(rule)
    } else {
        x11::close(rule)
    };
    match windows {
        Ok(Some(n)) if n > 0 => return Ok(Closed::Windows(n)),
        Err(e) => eprintln!("deckly: Close: cannot reach the windows: {e}"),
        _ => {}
    }
    Ok(Closed::Processes(end_processes(rule)))
}

pub fn limitation() -> Option<&'static str> {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    let supported = *SUPPORTED.get_or_init(|| !is_wayland() || wayland::supported());
    (!supported).then_some(NO_WINDOW_CLOSE)
}

/// Sends a termination signal to each of this user's processes that the rule
/// names, by program or script; Deckly is never one of them. Returns how many
/// took it.
fn end_processes(rule: &AppRule) -> usize {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return 0;
    };
    let own = std::process::id();
    let mut ended = 0;
    for entry in entries.take(MAX_ITEMS).flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == own || !rule.matches(&process_names(pid)) {
            continue;
        }
        let signalled = i32::try_from(pid)
            .ok()
            .and_then(rustix::process::Pid::from_raw)
            .is_some_and(|pid| {
                rustix::process::kill_process(pid, rustix::process::Signal::TERM).is_ok()
            });
        ended += usize::from(signalled);
    }
    ended
}

/// The longest command line read; only its first two parts count.
const MAX_COMMAND_BYTES: u64 = 64 * 1024;

fn process_names(pid: u32) -> Vec<String> {
    use std::io::Read;
    let mut names: Vec<String> = program(pid).into_iter().collect();
    let mut args = Vec::new();
    if let Ok(file) = std::fs::File::open(format!("/proc/{pid}/cmdline")) {
        let _ = file.take(MAX_COMMAND_BYTES).read_to_end(&mut args);
    }
    names.extend(command_names(&args));
    names
}

mod x11 {
    use super::*;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, Window,
    };

    struct Atoms {
        clients: Atom,
        pid: Atom,
        supported: Atom,
        net_close: Atom,
        protocols: Atom,
        delete: Atom,
    }

    /// How many windows closed; `None` without a window manager list.
    pub fn close(rule: &AppRule) -> Result<Option<usize>, Box<dyn Error>> {
        let (connection, screen) = x11rb::connect(None)?;
        let root = connection
            .setup()
            .roots
            .get(screen)
            .ok_or("no X11 screen")?
            .root;
        let atom = |name: &[u8]| -> Result<Atom, Box<dyn Error>> {
            Ok(connection.intern_atom(false, name)?.reply()?.atom)
        };
        let atoms = Atoms {
            clients: atom(b"_NET_CLIENT_LIST")?,
            pid: atom(b"_NET_WM_PID")?,
            supported: atom(b"_NET_SUPPORTED")?,
            net_close: atom(b"_NET_CLOSE_WINDOW")?,
            protocols: atom(b"WM_PROTOCOLS")?,
            delete: atom(b"WM_DELETE_WINDOW")?,
        };
        let limit = MAX_ITEMS as u32;
        let list = |window: Window, property: Atom, kind: AtomEnum| {
            connection
                .get_property(false, window, property, kind, 0, limit)
                .map_err(Box::<dyn Error>::from)
                .and_then(|cookie| gone_is_none(cookie.reply()))
                .map(|reply| {
                    reply
                        .and_then(|r| r.value32().map(Iterator::collect::<Vec<u32>>))
                        .unwrap_or_default()
                })
        };
        let windows = list(root, atoms.clients, AtomEnum::WINDOW)?;
        if windows.is_empty() {
            return Ok(None);
        }
        let by_manager = list(root, atoms.supported, AtomEnum::ATOM)?.contains(&atoms.net_close);
        let own = std::process::id();
        let mut closed = 0;
        for window in windows {
            let class = connection.get_property(
                false,
                window,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                0,
                64,
            )?;
            let mut names = gone_is_none(class.reply())?
                .map(|reply| class_names(&reply.value))
                .unwrap_or_default();
            let pid = list(window, atoms.pid, AtomEnum::CARDINAL)?
                .first()
                .copied();
            if pid == Some(own) {
                continue;
            }
            names.extend(pid.and_then(program));
            if !rule.matches(&names) {
                continue;
            }
            let event = if by_manager {
                // Source 2: a pager or task bar, acting for the user.
                ClientMessageEvent::new(32, window, atoms.net_close, [0, 2, 0, 0, 0])
            } else if list(window, atoms.protocols, AtomEnum::ATOM)?.contains(&atoms.delete) {
                ClientMessageEvent::new(32, window, atoms.protocols, [atoms.delete, 0, 0, 0, 0])
            } else {
                continue;
            };
            let (target, mask) = if by_manager {
                (
                    root,
                    EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                )
            } else {
                (window, EventMask::NO_EVENT)
            };
            connection.send_event(false, target, mask, event)?;
            closed += 1;
        }
        // Waits until the server has every request.
        connection.get_input_focus()?.reply()?;
        Ok(Some(closed))
    }
}

mod wayland {
    use super::*;
    use std::collections::HashMap;
    use wayland_client::backend::ObjectId;
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::wl_registry;
    use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
    use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::{
        self, ZwlrForeignToplevelHandleV1,
    };
    use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::{
        self, ZwlrForeignToplevelManagerV1,
    };

    /// Deckly's own windows, which the protocol names by application id only.
    const OWN_IDS: [&str; 2] = ["deckly", "deckly-web"];

    #[derive(Default)]
    struct Windows {
        handles: HashMap<ObjectId, (ZwlrForeignToplevelHandleV1, String)>,
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Windows {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for Windows {
        fn event(
            _: &mut Self,
            _: &ZwlrForeignToplevelManagerV1,
            _: zwlr_foreign_toplevel_manager_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }

        event_created_child!(Windows, ZwlrForeignToplevelManagerV1, [
            zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
        ]);
    }

    impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for Windows {
        fn event(
            windows: &mut Self,
            handle: &ZwlrForeignToplevelHandleV1,
            event: zwlr_foreign_toplevel_handle_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            use zwlr_foreign_toplevel_handle_v1::Event;
            match event {
                Event::AppId { app_id } if windows.handles.len() < MAX_ITEMS => {
                    windows
                        .handles
                        .insert(handle.id(), (handle.clone(), app_id));
                }
                Event::Closed => {
                    windows.handles.remove(&handle.id());
                }
                _ => {}
            }
        }
    }

    /// Whether the compositor offers the protocol.
    pub fn supported() -> bool {
        Connection::connect_to_env()
            .ok()
            .and_then(|c| registry_queue_init::<Windows>(&c).ok())
            .is_some_and(|(globals, queue)| {
                globals
                    .bind::<ZwlrForeignToplevelManagerV1, _, _>(&queue.handle(), 1..=3, ())
                    .is_ok()
            })
    }

    /// How many windows closed; `None` when the compositor cannot do it.
    pub fn close(rule: &AppRule) -> Result<Option<usize>, Box<dyn Error>> {
        let connection = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<Windows>(&connection)?;
        let manager: Result<ZwlrForeignToplevelManagerV1, _> =
            globals.bind(&queue.handle(), 1..=3, ());
        let Ok(manager) = manager else {
            return Ok(None);
        };
        let mut windows = Windows::default();
        // The first lists the windows, the second their details.
        queue.roundtrip(&mut windows)?;
        queue.roundtrip(&mut windows)?;
        let mut closed = 0;
        for (handle, app_id) in windows.handles.values() {
            if OWN_IDS.contains(&app_id.as_str()) || !rule.matches(std::slice::from_ref(app_id)) {
                continue;
            }
            handle.close();
            closed += 1;
        }
        manager.stop();
        queue.roundtrip(&mut windows)?;
        Ok(Some(closed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Child, Command};
    use std::time::{Duration, Instant};

    fn rule(id: &str) -> AppRule {
        AppRule::new("", id)
    }

    /// Waits for the child to end, and says how.
    fn ended(child: &mut Child, within: Duration) -> Option<std::process::ExitStatus> {
        let start = Instant::now();
        while start.elapsed() < within {
            if let Ok(Some(status)) = child.try_wait() {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        None
    }

    #[test]
    fn a_program_without_a_window_is_asked_to_end() {
        let dir = std::env::temp_dir().join(format!("deckly-close-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("deckly-close-sleeper");
        std::fs::copy("/bin/sleep", &program).unwrap();
        let mut child = Command::new(&program).arg("30").spawn().unwrap();
        // The program runs once its file is open; until then it is the test.
        let start = Instant::now();
        while process_names(child.id())
            .first()
            .is_none_or(|exe| !exe.ends_with("deckly-close-sleeper"))
            && start.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(end_processes(&rule("deckly-close-sleeper")), 1);
        let status = ended(&mut child, Duration::from_secs(5)).unwrap();
        assert_eq!(
            status.signal(),
            Some(rustix::process::Signal::TERM.as_raw())
        );
        assert_eq!(end_processes(&rule("deckly-close-sleeper")), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Opens a kitty window with its own class, closes it the way the session
    /// does it, and waits for kitty to end.
    fn closes_a_kitty_window(x11: bool) {
        let class = format!("deckly-close-test-{}", std::process::id());
        let mut command = Command::new("kitty");
        command.args(["--class", &class, "-o", "confirm_os_window_close=0"]);
        if x11 {
            command
                .env_remove("WAYLAND_DISPLAY")
                .args(["-o", "linux_display_server=x11"]);
        }
        let mut child = command.spawn().unwrap();
        let start = Instant::now();
        let mut closed = 0;
        while closed == 0 && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(200));
            let result = if x11 {
                x11::close(&rule(&class))
            } else {
                wayland::close(&rule(&class))
            };
            closed = result.unwrap().unwrap();
        }
        assert_eq!(closed, 1);
        let status = ended(&mut child, Duration::from_secs(5)).expect("kitty did not close");
        // Closed as by its button, not by a signal.
        assert_eq!(status.signal(), None, "{status:?}");
    }

    #[test]
    #[ignore = "opens a kitty window on a wlr Wayland desktop"]
    fn closes_a_wayland_window() {
        closes_a_kitty_window(false);
    }

    #[test]
    #[ignore = "opens a kitty window on X11 or XWayland"]
    fn closes_an_x11_window() {
        closes_a_kitty_window(true);
    }
}
