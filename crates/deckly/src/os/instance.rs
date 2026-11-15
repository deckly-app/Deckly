//! One Deckly at a time. A closed window leaves Deckly running in the tray,
//! so starting the program again must bring that window back instead of
//! opening the devices a second time.
//!
//! The first instance holds a lock on a file in the settings folder; the
//! system releases it however the process ends. It also listens on a local
//! port, which a later start connects to. A connection that carries no data
//! only shows the window.
//!
//! The plugin tool (`deckly-cli`) uses the same port to restart a plugin
//! after a build: it sends one control line (see
//! `deckly_plugin_host::proto::dev`) with the token Deckly writes at start
//! into a file only the user can read. Restarting plugins is more than
//! showing a window, so a line without that token is refused.

use deckly_plugin_host::proto::dev::{self, Control};
use std::fs::{self, File, TryLockError};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

const LOCK: &str = "instance.lock";
/// The lock is mandatory on Windows, so the port has its own file.
const PORT: &str = dev::PORT_FILE;
/// How long a connection may take to send its control line; one that
/// sends nothing in time is a later start asking for the window.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

pub struct Instance {
    lock: File,
    listener: TcpListener,
    token: String,
}

/// Returns `None` when another Deckly runs; it was asked to show its window.
///
/// A Deckly started by an update waits up to `wait` for the old one to
/// quit and release the lock, instead of showing the old window.
pub fn claim(dir: &Path, wait: Duration) -> io::Result<Option<Instance>> {
    fs::create_dir_all(dir)?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LOCK))?;
    let deadline = std::time::Instant::now() + wait;
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(TryLockError::WouldBlock) => {
                show_first(dir);
                return Ok(None);
            }
            Err(TryLockError::Error(e)) => return Err(e),
        }
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let token = uuid::Uuid::new_v4().simple().to_string();
    write_private(&dir.join(dev::TOKEN_FILE), &token)?;
    fs::write(dir.join(PORT), listener.local_addr()?.port().to_string())?;
    Ok(Some(Instance {
        lock,
        listener,
        token,
    }))
}

/// Writes a file only its owner can read (on Unix; on Windows the settings
/// folder is the user's own).
fn write_private(path: &Path, text: &str) -> io::Result<()> {
    let _ = fs::remove_file(path);
    let mut options = File::options();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

/// A first instance still starting has no port yet and misses this; the user
/// then sees its window open anyway.
fn show_first(dir: &Path) {
    let port = fs::read_to_string(dir.join(PORT))
        .ok()
        .and_then(|text| text.trim().parse::<u16>().ok());
    if let Some(port) = port {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let _ = TcpStream::connect_timeout(&address, Duration::from_secs(1));
    }
}

/// What a connection asks for.
enum Request {
    Show,
    Control(Control),
    /// A control line that cannot be served, with the answer.
    Refused(String),
}

/// Reads one connection: nothing (a later start) or a control line.
fn read_request(stream: &TcpStream, token: &str) -> Request {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let mut line = String::new();
    let limit = dev::MAX_CONTROL_BYTES as u64 + 1;
    let read = BufReader::new(stream.take(limit)).read_line(&mut line);
    if line.is_empty() {
        // A later start sends nothing; a read error or timeout counts too.
        return Request::Show;
    }
    if read.is_err() || !line.ends_with('\n') {
        return Request::Refused("the line was cut off".into());
    }
    match Control::parse(&line) {
        Ok((sent, control)) if same(sent.as_bytes(), token.as_bytes()) => Request::Control(control),
        Ok(_) => Request::Refused(format!(
            "wrong token; the tool reads it from {} in Deckly's settings folder",
            dev::TOKEN_FILE
        )),
        Err(e) => Request::Refused(e),
    }
}

/// Compares two tokens in time that does not depend on where they differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

impl Instance {
    /// Calls `show` for each later start, and `control` for each request
    /// from the plugin tool, for as long as the process runs. Requests are
    /// served one at a time.
    pub fn listen(
        self,
        show: impl Fn() + Send + 'static,
        control: impl Fn(Control) -> Result<String, String> + Send + 'static,
    ) {
        let run = move || {
            let _lock = self.lock;
            for stream in self.listener.incoming() {
                let Ok(mut stream) = stream else {
                    continue;
                };
                let result = match read_request(&stream, &self.token) {
                    Request::Show => {
                        show();
                        continue;
                    }
                    Request::Control(c) => control(c),
                    Request::Refused(e) => Err(e),
                };
                let _ = stream.write_all(dev::answer(&result).as_bytes());
            }
        };
        if let Err(e) = std::thread::Builder::new()
            .name("deckly-instance".into())
            .spawn(run)
        {
            eprintln!("deckly: cannot watch for later starts: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn folder(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("deckly-{name}-{}", std::process::id()))
    }

    #[test]
    fn a_second_start_shows_the_first() {
        let dir = folder("second-start");
        let first = claim(&dir, Duration::ZERO).unwrap().unwrap();
        let (tx, rx) = channel();
        first.listen(
            move || {
                let _ = tx.send(());
            },
            |_| Err("not here".into()),
        );
        assert!(claim(&dir, Duration::ZERO).unwrap().is_none());
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    /// Sends a line as the plugin tool does and reads the answer.
    fn ask(dir: &Path, line: &str) -> Result<String, String> {
        let port: u16 = fs::read_to_string(dir.join(PORT))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream.write_all(line.as_bytes()).unwrap();
        let mut answer = String::new();
        BufReader::new(stream).read_line(&mut answer).unwrap();
        dev::parse_answer(&answer)
    }

    #[test]
    fn the_plugin_tool_restarts_plugins_with_the_token() {
        let dir = folder("control");
        let first = claim(&dir, Duration::ZERO).unwrap().unwrap();
        let (tx, rx) = channel();
        first.listen(
            || panic!("a control line does not show the window"),
            move |c| {
                let _ = tx.send(c);
                Ok("restarted".into())
            },
        );
        let token = fs::read_to_string(dir.join(dev::TOKEN_FILE)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join(dev::TOKEN_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "{mode:o}");
        }
        let reload = Control::Reload("com.example.hello".into());
        assert_eq!(ask(&dir, &reload.line(&token)), Ok("restarted".into()));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), reload);
        let wrong = ask(&dir, &reload.line("not-the-token")).unwrap_err();
        assert!(wrong.contains("token"), "{wrong}");
        assert!(ask(&dir, "deckly-control 1 x format c:\n").is_err());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_released_lock_can_be_claimed_again() {
        let dir = folder("released");
        drop(claim(&dir, Duration::ZERO).unwrap().unwrap());
        assert!(claim(&dir, Duration::ZERO).unwrap().is_some());
    }

    #[test]
    fn an_updated_start_waits_for_the_old_one_to_quit() {
        let dir = folder("after-update");
        let first = claim(&dir, Duration::ZERO).unwrap().unwrap();
        let quit = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(first);
        });
        assert!(claim(&dir, Duration::from_secs(10)).unwrap().is_some());
        quit.join().unwrap();
    }

    #[test]
    fn a_damaged_port_file_does_not_stop_the_second_start() {
        let dir = folder("damaged-port");
        let _first = claim(&dir, Duration::ZERO).unwrap().unwrap();
        fs::write(dir.join(PORT), "not a port").unwrap();
        assert!(claim(&dir, Duration::ZERO).unwrap().is_none());
    }
}
