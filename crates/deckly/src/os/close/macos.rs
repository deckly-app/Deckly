//! macOS: a running application is asked to quit, as Quit in its menu would;
//! it may still ask about unsaved work. A program that is no application (a
//! script, a command-line tool) gets a termination signal through `pkill`,
//! which matches the process name the system keeps, at most 16 characters.

use super::Closed;
use crate::os::active_window::macos::names;
use deckly_core::AppRule;
use objc2_app_kit::NSWorkspace;
use std::process::{Command, Stdio};

pub fn close(rule: &AppRule) -> Result<Closed, String> {
    let own = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.to_str().map(str::to_owned));
    let mut asked = 0;
    for app in NSWorkspace::sharedWorkspace().runningApplications().iter() {
        let names = names(&app);
        let own_app = own.as_ref().is_some_and(|exe| names.contains(exe));
        if !own_app && rule.matches(&names) && app.terminate() {
            asked += 1;
        }
    }
    if asked > 0 {
        return Ok(Closed::Windows(asked));
    }
    Ok(Closed::Processes(end_processes(rule)))
}

/// `pkill` ends only this user's processes and exits with 0 when one matched.
fn end_processes(rule: &AppRule) -> usize {
    let Some(uid) = user_id() else {
        return 0;
    };
    let status = Command::new("/usr/bin/pkill")
        .args(["-TERM", "-x", "-U", &uid, "--", &rule.id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    usize::from(status.is_ok_and(|s| s.success()))
}

/// This user's id, from the system rather than the environment.
fn user_id() -> Option<String> {
    Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|uid| uid.trim().to_owned())
        .filter(|uid| !uid.is_empty() && uid.bytes().all(|b| b.is_ascii_digit()))
}
