//! Talking to a running Deckly: one control line over the port it listens
//! on, with the token it keeps next to it (see
//! `deckly_plugin_protocol::dev`).

use deckly_plugin_protocol::dev::{
    Control, MAX_CONTROL_BYTES, PORT_FILE, TOKEN_FILE, parse_answer,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// A restart stops the old process (up to a few seconds) and copies the
/// plugin folder.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

/// What a running Deckly answered.
#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    Done(String),
    /// No Deckly runs with this settings folder.
    NotRunning,
}

/// Sends `control` to the Deckly that uses `config`. An error is Deckly's
/// own answer, or a reason it could not be asked.
pub fn send(config: &Path, control: &Control) -> Result<Answer, String> {
    let Some(port) = std::fs::read_to_string(config.join(PORT_FILE))
        .ok()
        .and_then(|t| t.trim().parse::<u16>().ok())
    else {
        return Ok(Answer::NotRunning);
    };
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) else {
        // The port file outlives a Deckly that quit.
        return Ok(Answer::NotRunning);
    };
    let token = std::fs::read_to_string(config.join(TOKEN_FILE)).map_err(|e| {
        format!(
            "Deckly runs, but its {TOKEN_FILE} cannot be read ({e}); \
             a Deckly older than this tool cannot restart plugins on request"
        )
    })?;
    stream
        .set_read_timeout(Some(ANSWER_TIMEOUT))
        .and_then(|()| stream.write_all(control.line(token.trim()).as_bytes()))
        .map_err(|e| format!("cannot talk to Deckly: {e}"))?;
    let mut line = String::new();
    BufReader::new(stream.take(MAX_CONTROL_BYTES as u64))
        .read_line(&mut line)
        .map_err(|e| format!("Deckly did not answer: {e}"))?;
    if line.is_empty() {
        return Err(
            "Deckly closed the connection without an answer; it may be older than this tool".into(),
        );
    }
    parse_answer(&line).map(Answer::Done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckly_plugin_protocol::dev::answer;
    use std::net::TcpListener;

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("deckly-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn no_port_or_a_closed_port_means_not_running() {
        let dir = folder("not-running");
        let reload = Control::Reload("com.example.a".into());
        assert_eq!(send(&dir, &reload), Ok(Answer::NotRunning));
        // A port nobody listens on any more.
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        std::fs::write(dir.join(PORT_FILE), port.to_string()).unwrap();
        assert_eq!(send(&dir, &reload), Ok(Answer::NotRunning));
    }

    /// A fake Deckly that checks the token and answers.
    #[test]
    fn sends_the_line_with_the_token() {
        let dir = folder("control");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::fs::write(dir.join(PORT_FILE), port.to_string()).unwrap();
        std::fs::write(dir.join(TOKEN_FILE), "secret\n").unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let (token, control) = Control::parse(&line).unwrap();
            assert_eq!(token, "secret");
            assert_eq!(control, Control::Reload("com.example.a".into()));
            stream
                .write_all(answer(&Ok("restarted".into())).as_bytes())
                .unwrap();
        });
        let reload = Control::Reload("com.example.a".into());
        assert_eq!(send(&dir, &reload), Ok(Answer::Done("restarted".into())));
        server.join().unwrap();
    }
}
