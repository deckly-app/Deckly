//! The wizard's questions: a line of text with a suggestion to edit, a
//! choice from a short list, and yes or no.
//!
//! The prompts are drawn so that terminals with an animated cursor (kitty's
//! `cursor_trail`, Neovide, WezTerm and others) stay calm. Every redraw is
//! one write: the cursor is hidden, the lines are rewritten in place, and the
//! cursor is shown once, at its final place, inside a synchronized update
//! (`CSI ? 2026 h/l`, which terminals that do not know it ignore). A
//! prompt that has nothing to type keeps the cursor hidden. An unchanged
//! frame writes nothing.
//!
//! Nothing here depends on the shell the tool was started from: the keys
//! come from the terminal in raw mode, bracketed paste is switched on and
//! off again, and whatever happens (an answer, Esc, Ctrl+C, an error) the
//! terminal is given back with the cursor shown, in cooked mode, and the
//! next output starts on a new line.

use crate::term;
use crossterm::cursor::{Hide, MoveDown, MoveToColumn, MoveUp, Show};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::style::Print;
use crossterm::terminal::{self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate};
use crossterm::{execute, queue};
use std::fmt;
use std::io::{self, Write};
use unicode_width::UnicodeWidthChar;

/// The most characters one answer may hold; a paste is cut there.
const MAX_CHARS: usize = 512;

#[derive(Debug)]
pub enum PromptError {
    /// Esc or Ctrl+C.
    Cancelled,
    Io(io::Error),
}

impl fmt::Display for PromptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PromptError::Cancelled => f.write_str("stopped; nothing was written"),
            PromptError::Io(e) => write!(f, "cannot ask: {e}"),
        }
    }
}

impl From<io::Error> for PromptError {
    fn from(e: io::Error) -> Self {
        PromptError::Io(e)
    }
}

/// A line of text, with suggestion `initial` filled in to edit.
pub fn text(
    label: &str,
    initial: &str,
    hint: Option<&str>,
    validate: &dyn Fn(&str) -> Result<(), String>,
) -> Result<String, PromptError> {
    let mut p = TextPrompt {
        label,
        line: Line::new(initial),
        hint,
        validate,
        error: None,
    };
    drive(&mut p)?;
    Ok(p.line.text())
}

/// One of a few options, the first one shown as `(value, description)`.
pub fn select<T: Copy>(label: &str, options: &[(T, &str)]) -> Result<T, PromptError> {
    let texts: Vec<&str> = options.iter().map(|(_, text)| *text).collect();
    let mut p = SelectPrompt {
        label,
        options: &texts,
        index: 0,
    };
    drive(&mut p)?;
    options
        .get(p.index)
        .map(|(value, _)| *value)
        .ok_or_else(|| PromptError::Io(io::Error::other("no options to choose from")))
}

/// Yes or no; Enter takes `default`.
pub fn confirm(label: &str, default: bool) -> Result<bool, PromptError> {
    let mut p = ConfirmPrompt {
        label,
        value: default,
    };
    drive(&mut p)?;
    Ok(p.value)
}

// ---------------------------------------------------------------------
// The loop

enum Step {
    Continue,
    Submit,
    Cancel,
}

trait Prompt {
    /// The prompt as it is drawn while it waits.
    fn frame(&self, width: usize) -> Frame;
    /// The one line that stays once it is answered.
    fn done(&self, width: usize) -> Frame;
    fn key(&mut self, key: KeyEvent) -> Step;
    fn paste(&mut self, _text: &str) {}
}

enum Input {
    Key(KeyEvent),
    Paste(String),
    Resize,
}

fn drive<P: Prompt>(prompt: &mut P) -> Result<(), PromptError> {
    let _raw = RawMode::enter()?;
    let mut screen = Screen::new(io::stdout());
    loop {
        screen.draw(&prompt.frame(term::columns()))?;
        match read_input()? {
            Input::Key(key) => match prompt.key(key) {
                Step::Continue => {}
                Step::Submit => {
                    screen.finish(&prompt.done(term::columns()))?;
                    return Ok(());
                }
                Step::Cancel => {
                    screen.leave(true)?;
                    return Err(PromptError::Cancelled);
                }
            },
            Input::Paste(text) => prompt.paste(&text),
            Input::Resize => screen.resized()?,
        }
    }
}

/// The next thing the user did. Blocks; key releases (Windows, and kitty's
/// keyboard protocol) are not input.
fn read_input() -> io::Result<Input> {
    loop {
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => return Ok(Input::Key(key)),
            Event::Paste(text) => return Ok(Input::Paste(text)),
            Event::Resize(..) => return Ok(Input::Resize),
            _ => {}
        }
    }
}

/// Raw mode with bracketed paste, undone when it goes out of scope.
struct RawMode;

impl RawMode {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        // Not every terminal has bracketed paste; a paste then arrives as
        // keys, which works too.
        let _ = execute!(io::stdout(), EnableBracketedPaste);
        Ok(RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableBracketedPaste, Show);
        let _ = terminal::disable_raw_mode();
    }
}

// ---------------------------------------------------------------------
// Drawing

/// What the prompt looks like: lines (already coloured and no wider than
/// the terminal) and where the cursor goes, if it is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Frame {
    lines: Vec<String>,
    /// Row (in `lines`) and column; `None` keeps the cursor hidden.
    cursor: Option<(usize, usize)>,
}

/// Draws frames one over the other, where the first one began.
struct Screen<W: Write> {
    out: W,
    shown: Option<Frame>,
    /// The row of the cursor, counted from the first line of the frame.
    row: usize,
}

fn cells(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

impl<W: Write> Screen<W> {
    fn new(out: W) -> Self {
        Screen {
            out,
            shown: None,
            row: 0,
        }
    }

    fn draw(&mut self, frame: &Frame) -> io::Result<()> {
        if self.shown.as_ref() == Some(frame) {
            return Ok(());
        }
        let mut buf: Vec<u8> = Vec::with_capacity(512);
        queue!(buf, BeginSynchronizedUpdate, Hide)?;
        if self.row > 0 {
            queue!(buf, MoveUp(cells(self.row)))?;
        }
        queue!(buf, MoveToColumn(0))?;
        let count = frame.lines.len();
        for (i, line) in frame.lines.iter().enumerate() {
            queue!(buf, Clear(ClearType::CurrentLine), Print(line))?;
            if i + 1 < count {
                queue!(buf, Print("\r\n"))?;
            }
        }
        // What an earlier, taller frame left below.
        queue!(buf, Clear(ClearType::FromCursorDown))?;
        let last = count.saturating_sub(1);
        let row = match frame.cursor {
            Some((row, column)) => {
                if last > row {
                    queue!(buf, MoveUp(cells(last - row)))?;
                }
                queue!(buf, MoveToColumn(cells(column)), Show)?;
                row
            }
            None => last,
        };
        queue!(buf, EndSynchronizedUpdate)?;
        self.out.write_all(&buf)?;
        self.out.flush()?;
        self.row = row;
        self.shown = Some(frame.clone());
        Ok(())
    }

    /// Leaves the answered line behind and moves on to a new line.
    fn finish(&mut self, done: &Frame) -> io::Result<()> {
        self.draw(done)?;
        self.leave(true)
    }

    /// Moves below the frame, to the start of a new line.
    fn leave(&mut self, show_cursor: bool) -> io::Result<()> {
        let mut buf: Vec<u8> = Vec::new();
        if let Some(frame) = &self.shown {
            let last = frame.lines.len().saturating_sub(1);
            if last > self.row {
                queue!(buf, MoveDown(cells(last - self.row)))?;
            }
        }
        queue!(buf, Print("\r\n"))?;
        if show_cursor {
            queue!(buf, Show)?;
        }
        self.out.write_all(&buf)?;
        self.out.flush()?;
        self.shown = None;
        self.row = 0;
        Ok(())
    }

    /// The terminal changed size, and what it did to the lines already
    /// drawn is not known: the next frame starts below them.
    fn resized(&mut self) -> io::Result<()> {
        if self.shown.is_some() {
            self.leave(false)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Widths

fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

fn text_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

/// `text` cut to `max` columns, with "..." where it was cut if asked to.
fn clip(text: &str, max: usize, dots: bool) -> String {
    if text_width(text) <= max {
        return text.to_owned();
    }
    let dots = if dots && max >= 3 { "..." } else { "" };
    let room = max - dots.len();
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = char_width(c);
        if used + w > room {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push_str(dots);
    out
}

/// The columns a line may use: one is kept free, so a full line never
/// leaves the terminal waiting to wrap.
fn usable(width: usize) -> usize {
    width.saturating_sub(1).max(10)
}

// ---------------------------------------------------------------------
// A line of text

/// The text being edited and the place of the cursor, in characters.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Line {
    chars: Vec<char>,
    cursor: usize,
}

impl Line {
    fn new(initial: &str) -> Self {
        let mut line = Line {
            chars: Vec::new(),
            cursor: 0,
        };
        line.insert_str(initial);
        line
    }

    fn text(&self) -> String {
        self.chars.iter().collect()
    }

    fn insert(&mut self, c: char) {
        if c.is_control() || self.chars.len() >= MAX_CHARS {
            return;
        }
        self.chars.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Text typed in one go; a line break ends it.
    fn insert_str(&mut self, text: &str) {
        for c in text.chars().take_while(|c| *c != '\n' && *c != '\r') {
            self.insert(c);
        }
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    fn kill_to_start(&mut self) {
        self.chars.drain(..self.cursor);
        self.cursor = 0;
    }

    fn kill_to_end(&mut self) {
        self.chars.truncate(self.cursor);
    }

    /// Ctrl+W: the spaces before the cursor and the word before them.
    fn kill_word(&mut self) {
        let mut start = self.cursor;
        while start > 0 && self.chars.get(start - 1).is_some_and(|c| c.is_whitespace()) {
            start -= 1;
        }
        while start > 0
            && self
                .chars
                .get(start - 1)
                .is_some_and(|c| !c.is_whitespace())
        {
            start -= 1;
        }
        self.chars.drain(start..self.cursor);
        self.cursor = start;
    }

    fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars.len());
    }

    fn word_left(&mut self) {
        let mut at = self.cursor;
        while at > 0 && self.chars.get(at - 1).is_some_and(|c| c.is_whitespace()) {
            at -= 1;
        }
        while at > 0 && self.chars.get(at - 1).is_some_and(|c| !c.is_whitespace()) {
            at -= 1;
        }
        self.cursor = at;
    }

    fn word_right(&mut self) {
        let mut at = self.cursor;
        while self.chars.get(at).is_some_and(|c| !c.is_whitespace()) {
            at += 1;
        }
        while self.chars.get(at).is_some_and(|c| c.is_whitespace()) {
            at += 1;
        }
        self.cursor = at;
    }
}

/// What a key did to a line of text.
enum Edit {
    Changed,
    Moved,
    Enter,
    Cancel,
    Nothing,
}

fn edit_key(line: &mut Line, key: KeyEvent) -> Edit {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Enter => Edit::Enter,
        KeyCode::Esc => Edit::Cancel,
        KeyCode::Char('c') if ctrl && !alt => Edit::Cancel,
        KeyCode::Backspace if alt || ctrl => {
            line.kill_word();
            Edit::Changed
        }
        KeyCode::Backspace => {
            line.backspace();
            Edit::Changed
        }
        KeyCode::Delete => {
            line.delete();
            Edit::Changed
        }
        KeyCode::Left if ctrl || alt => {
            line.word_left();
            Edit::Moved
        }
        KeyCode::Right if ctrl || alt => {
            line.word_right();
            Edit::Moved
        }
        KeyCode::Left => {
            line.left();
            Edit::Moved
        }
        KeyCode::Right => {
            line.right();
            Edit::Moved
        }
        KeyCode::Home => {
            line.cursor = 0;
            Edit::Moved
        }
        KeyCode::End => {
            line.cursor = line.chars.len();
            Edit::Moved
        }
        KeyCode::Char(c) if ctrl && !alt => match c.to_ascii_lowercase() {
            'a' => {
                line.cursor = 0;
                Edit::Moved
            }
            'e' => {
                line.cursor = line.chars.len();
                Edit::Moved
            }
            'b' => {
                line.left();
                Edit::Moved
            }
            'f' => {
                line.right();
                Edit::Moved
            }
            'u' => {
                line.kill_to_start();
                Edit::Changed
            }
            'k' => {
                line.kill_to_end();
                Edit::Changed
            }
            'w' => {
                line.kill_word();
                Edit::Changed
            }
            'd' => {
                line.delete();
                Edit::Changed
            }
            'h' => {
                line.backspace();
                Edit::Changed
            }
            _ => Edit::Nothing,
        },
        // Ctrl and Alt together is how Windows reports AltGr.
        KeyCode::Char(c) if !(ctrl || alt) || (ctrl && alt) => {
            line.insert(c);
            Edit::Changed
        }
        _ => Edit::Nothing,
    }
}

struct TextPrompt<'a> {
    label: &'a str,
    line: Line,
    hint: Option<&'a str>,
    validate: &'a dyn Fn(&str) -> Result<(), String>,
    error: Option<String>,
}

impl Prompt for TextPrompt<'_> {
    fn frame(&self, width: usize) -> Frame {
        text_frame(
            self.label,
            &self.line,
            self.hint,
            self.error.as_deref(),
            width,
        )
    }

    fn done(&self, width: usize) -> Frame {
        text_frame(self.label, &self.line, None, None, width).without_cursor()
    }

    fn key(&mut self, key: KeyEvent) -> Step {
        match edit_key(&mut self.line, key) {
            Edit::Cancel => Step::Cancel,
            Edit::Enter => match (self.validate)(&self.line.text()) {
                Ok(()) => Step::Submit,
                Err(message) => {
                    self.error = Some(message);
                    Step::Continue
                }
            },
            Edit::Changed => {
                self.error = None;
                Step::Continue
            }
            Edit::Moved | Edit::Nothing => Step::Continue,
        }
    }

    fn paste(&mut self, text: &str) {
        self.line.insert_str(text);
        self.error = None;
    }
}

impl Frame {
    fn without_cursor(mut self) -> Self {
        self.cursor = None;
        self
    }
}

/// `? Label text`, then the hint or the error under it. A text longer than
/// the terminal scrolls to keep the cursor in view.
fn text_frame(
    label: &str,
    line: &Line,
    hint: Option<&str>,
    error: Option<&str>,
    width: usize,
) -> Frame {
    let usable = usable(width);
    let label = clip(label, usable / 2, false);
    let head = 2 + text_width(&label) + 1;
    let room = usable.saturating_sub(head).max(1);
    // The first character shown: the cursor must fit in `room`.
    let mut start = 0;
    while start < line.cursor
        && line
            .chars
            .get(start..line.cursor)
            .map_or(0, |s| s.iter().copied().map(char_width).sum::<usize>())
            > room
    {
        start += 1;
    }
    let mut shown = String::new();
    let mut used = 0;
    let mut before_cursor = 0;
    for (i, c) in line.chars.iter().enumerate().skip(start) {
        let w = char_width(*c);
        if used + w > room {
            break;
        }
        if i < line.cursor {
            before_cursor += w;
        }
        used += w;
        shown.push(*c);
    }
    let mut lines = vec![format!(
        "{} {} {}",
        term::green("?"),
        term::bold(&label),
        term::cyan(&shown)
    )];
    if let Some(message) = error {
        lines.push(term::red(format!(
            "  ! {}",
            clip(message, usable - 4, true)
        )));
    } else if let Some(hint) = hint {
        lines.push(term::dim(format!("  {}", clip(hint, usable - 2, true))));
    }
    Frame {
        lines,
        cursor: Some((0, head + before_cursor)),
    }
}

// ---------------------------------------------------------------------
// A choice

struct SelectPrompt<'a> {
    label: &'a str,
    options: &'a [&'a str],
    index: usize,
}

impl Prompt for SelectPrompt<'_> {
    fn frame(&self, width: usize) -> Frame {
        let usable = usable(width);
        let label = clip(self.label, usable / 2, false);
        let mut lines = vec![format!(
            "{} {} {}",
            term::green("?"),
            term::bold(&label),
            term::dim("(Up and Down to move, Enter to choose)")
        )];
        for (i, option) in self.options.iter().enumerate() {
            let option = clip(option, usable - 2, true);
            lines.push(if i == self.index {
                term::cyan(format!("> {option}"))
            } else {
                format!("  {option}")
            });
        }
        Frame {
            lines,
            cursor: None,
        }
    }

    fn done(&self, width: usize) -> Frame {
        let usable = usable(width);
        let label = clip(self.label, usable / 2, false);
        let head = 2 + text_width(&label) + 1;
        let option = self.options.get(self.index).copied().unwrap_or_default();
        Frame {
            lines: vec![format!(
                "{} {} {}",
                term::green("?"),
                term::bold(&label),
                term::cyan(clip(option, usable.saturating_sub(head), true))
            )],
            cursor: None,
        }
    }

    fn key(&mut self, key: KeyEvent) -> Step {
        let last = self.options.len().saturating_sub(1);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => return Step::Submit,
            KeyCode::Esc => return Step::Cancel,
            KeyCode::Char('c') if ctrl => return Step::Cancel,
            KeyCode::Up | KeyCode::Char('k') => self.index = self.index.saturating_sub(1),
            KeyCode::Char('p') if ctrl => self.index = self.index.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.index = (self.index + 1).min(last),
            KeyCode::Char('n') if ctrl => self.index = (self.index + 1).min(last),
            KeyCode::Home => self.index = 0,
            KeyCode::End => self.index = last,
            KeyCode::Char(digit @ '1'..='9') if !ctrl => {
                let n = usize::from(digit as u8 - b'1');
                if n <= last {
                    self.index = n;
                }
            }
            _ => {}
        }
        Step::Continue
    }
}

// ---------------------------------------------------------------------
// Yes or no

struct ConfirmPrompt<'a> {
    label: &'a str,
    value: bool,
}

impl Prompt for ConfirmPrompt<'_> {
    fn frame(&self, width: usize) -> Frame {
        let usable = usable(width);
        let label = clip(self.label, usable / 2, false);
        let choices = if self.value { "(Y/n)" } else { "(y/N)" };
        Frame {
            lines: vec![format!(
                "{} {} {}",
                term::green("?"),
                term::bold(&label),
                term::dim(choices)
            )],
            cursor: None,
        }
    }

    fn done(&self, width: usize) -> Frame {
        let usable = usable(width);
        let label = clip(self.label, usable / 2, false);
        let answer = if self.value { "Yes" } else { "No" };
        Frame {
            lines: vec![format!(
                "{} {} {}",
                term::green("?"),
                term::bold(&label),
                term::cyan(answer)
            )],
            cursor: None,
        }
    }

    fn key(&mut self, key: KeyEvent) -> Step {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => Step::Submit,
            KeyCode::Esc => Step::Cancel,
            KeyCode::Char('c') if ctrl => Step::Cancel,
            KeyCode::Char('y' | 'Y') if !ctrl => {
                self.value = true;
                Step::Submit
            }
            KeyCode::Char('n' | 'N') if !ctrl => {
                self.value = false;
                Step::Submit
            }
            _ => Step::Continue,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    /// The text without colour codes.
    fn plain(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for e in chars.by_ref() {
                    if e.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn frame_of(lines: &[&str], cursor: Option<(usize, usize)>) -> Frame {
        Frame {
            lines: lines.iter().map(|l| (*l).to_owned()).collect(),
            cursor,
        }
    }

    /// A terminal in a few lines: enough of the escape codes the screen
    /// writes to see what is left on it.
    #[derive(Default)]
    struct Emulator {
        rows: Vec<Vec<char>>,
        row: usize,
        column: usize,
        hidden: bool,
    }

    impl Emulator {
        fn feed(&mut self, bytes: &[u8]) {
            let text = String::from_utf8_lossy(bytes).into_owned();
            let mut chars = text.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '\r' => self.column = 0,
                    '\n' => self.row += 1,
                    '\x1b' => {
                        assert_eq!(chars.next(), Some('['));
                        let mut params = String::new();
                        let mut last = ' ';
                        for e in chars.by_ref() {
                            if e.is_ascii_alphabetic() {
                                last = e;
                                break;
                            }
                            params.push(e);
                        }
                        self.csi(&params, last);
                    }
                    c => {
                        let column = self.column;
                        let line = self.line();
                        if line.len() < column {
                            line.resize(column, ' ');
                        }
                        if column < line.len() {
                            line[column] = c;
                        } else {
                            line.push(c);
                        }
                        self.column += 1;
                    }
                }
            }
        }

        fn line(&mut self) -> &mut Vec<char> {
            while self.rows.len() <= self.row {
                self.rows.push(Vec::new());
            }
            &mut self.rows[self.row]
        }

        fn csi(&mut self, params: &str, last: char) {
            let n = params.trim_start_matches('?').parse::<usize>().unwrap_or(1);
            match (params, last) {
                (p, 'h' | 'l') if p.starts_with('?') => {
                    if p == "?25" {
                        self.hidden = last == 'l';
                    }
                }
                (_, 'A') => self.row = self.row.saturating_sub(n),
                (_, 'B') => self.row += n,
                (_, 'G') => self.column = n - 1,
                ("2", 'K') => self.line().clear(),
                ("", 'J') => {
                    let column = self.column;
                    self.line().truncate(column);
                    let keep = self.row + 1;
                    self.rows.truncate(keep);
                }
                (_, 'm') => {}
                other => panic!("the test terminal cannot do {other:?}"),
            }
        }

        fn screen(&self) -> Vec<String> {
            let mut lines: Vec<String> = self
                .rows
                .iter()
                .map(|r| r.iter().collect::<String>().trim_end().to_owned())
                .collect();
            while lines.last().is_some_and(String::is_empty) {
                lines.pop();
            }
            lines
        }
    }

    /// Draws the frames in order into the test terminal, which first has
    /// `before` on its screen; also returns what each draw wrote.
    fn draw_all(frames: &[Frame]) -> (Emulator, Vec<Vec<u8>>) {
        let mut sink: Vec<u8> = Vec::new();
        let mut screen = Screen::new(&mut sink);
        let mut writes = Vec::new();
        let mut emulator = Emulator::default();
        for frame in frames {
            screen.draw(frame).unwrap();
            let written = std::mem::take(screen.out);
            emulator.feed(&written);
            writes.push(written);
        }
        (emulator, writes)
    }

    fn find(haystack: &[u8], needle: &str) -> Option<usize> {
        String::from_utf8_lossy(haystack).find(needle)
    }

    #[test]
    fn a_redraw_is_one_write_with_the_cursor_hidden_while_it_moves() {
        let frames = [
            frame_of(&["? Author: a", "  hint"], Some((0, 11))),
            frame_of(&["? Author: ab", "  hint"], Some((0, 12))),
        ];
        let (_, writes) = draw_all(&frames);
        for w in &writes {
            let hide = find(w, "\x1b[?25l").unwrap();
            let show = find(w, "\x1b[?25h").unwrap();
            let begin = find(w, "\x1b[?2026h").unwrap();
            let end = find(w, "\x1b[?2026l").unwrap();
            assert!(begin < hide && hide < show && show < end, "{w:?}");
            // The cursor is shown once, and last before the update ends.
            assert_eq!(String::from_utf8_lossy(w).matches("\x1b[?25h").count(), 1);
        }
        // The second draw goes back up only while the cursor is hidden.
        let second = &writes[1];
        assert!(find(second, "\x1b[?25l").unwrap() < find(second, "A").unwrap());
        assert!(find(second, "A").unwrap() < find(second, "\x1b[?25h").unwrap());
    }

    #[test]
    fn a_frame_that_did_not_change_writes_nothing() {
        let f = frame_of(&["? Name: x"], Some((0, 9)));
        let (_, writes) = draw_all(&[f.clone(), f]);
        assert!(!writes[0].is_empty());
        assert!(writes[1].is_empty());
    }

    #[test]
    fn a_choice_keeps_the_cursor_hidden() {
        let (emulator, writes) = draw_all(&[frame_of(&["? Pick", "> a", "  b"], None)]);
        assert!(find(&writes[0], "\x1b[?25h").is_none());
        assert!(emulator.hidden);
    }

    #[test]
    fn a_shorter_frame_leaves_nothing_of_the_taller_one() {
        let (emulator, _) = draw_all(&[
            frame_of(&["? Name: x", "  ! At most 3.", "  extra"], Some((0, 9))),
            frame_of(&["? Name: xy", "  hint"], Some((0, 10))),
            frame_of(&["? Name: xy"], None),
        ]);
        assert_eq!(emulator.screen(), vec!["? Name: xy".to_owned()]);
    }

    #[test]
    fn answering_leaves_one_line_and_a_fresh_one_below() {
        let mut sink: Vec<u8> = Vec::new();
        let mut screen = Screen::new(&mut sink);
        screen
            .draw(&frame_of(&["? Name: x", "  hint"], Some((0, 9))))
            .unwrap();
        screen.finish(&frame_of(&["? Name: x"], None)).unwrap();
        let written = sink.clone();
        let mut emulator = Emulator::default();
        emulator.feed(&written);
        assert_eq!(emulator.screen(), vec!["? Name: x".to_owned()]);
        // The next output starts on the line after it, in column 0, and
        // the cursor is shown again.
        assert_eq!((emulator.row, emulator.column), (1, 0));
        assert!(!emulator.hidden);
    }

    #[test]
    fn a_resize_starts_the_next_frame_below_the_old_one() {
        let mut sink: Vec<u8> = Vec::new();
        let mut screen = Screen::new(&mut sink);
        screen
            .draw(&frame_of(&["? Name: x", "  hint"], Some((0, 9))))
            .unwrap();
        screen.resized().unwrap();
        screen
            .draw(&frame_of(&["? Name: xy", "  hint"], Some((0, 10))))
            .unwrap();
        let mut emulator = Emulator::default();
        emulator.feed(&sink);
        assert_eq!(
            emulator.screen(),
            vec!["? Name: x", "  hint", "? Name: xy", "  hint"]
        );
    }

    #[test]
    fn lines_never_reach_the_edge_of_the_terminal() {
        let wide = "\u{30c7}\u{30c3}\u{30ad}".repeat(30);
        for width in [10, 24, 40, 80] {
            for cursor_at in [0, 1, 20, 90] {
                let mut line = Line::new(&wide);
                line.cursor = cursor_at.min(line.chars.len());
                let f = text_frame(
                    "Plugin name:",
                    &line,
                    Some("Shown in Deckly's action list"),
                    Some("At most 64 characters, and then some more words to make it long."),
                    width,
                );
                for l in &f.lines {
                    assert!(text_width(&plain(l)) < width.max(11), "{width}: {l:?}");
                }
                let (row, column) = f.cursor.unwrap();
                assert_eq!(row, 0);
                assert!(column < width.max(11), "{width}: column {column}");
            }
        }
    }

    #[test]
    fn the_cursor_is_where_the_text_before_it_ends() {
        let mut line = Line::new("Ada");
        let f = text_frame("Author:", &line, None, None, 80);
        assert_eq!(plain(&f.lines[0]), "? Author: Ada");
        assert_eq!(f.cursor, Some((0, "? Author: Ada".len())));
        line.left();
        let f = text_frame("Author:", &line, None, None, 80);
        assert_eq!(f.cursor, Some((0, "? Author: Ad".len())));
        // A wide character takes two columns.
        let line = Line::new("\u{30c7}");
        let f = text_frame("A:", &line, None, None, 80);
        assert_eq!(f.cursor, Some((0, "? A: ".len() + 2)));
    }

    #[test]
    fn an_error_replaces_the_hint() {
        let line = Line::new("");
        let f = text_frame("Name:", &line, Some("Shown in the list"), None, 80);
        assert_eq!(plain(&f.lines[1]), "  Shown in the list");
        let f = text_frame(
            "Name:",
            &line,
            Some("Shown in the list"),
            Some("Give a name."),
            80,
        );
        assert_eq!(f.lines.len(), 2);
        assert_eq!(plain(&f.lines[1]), "  ! Give a name.");
    }

    #[test]
    fn a_line_is_edited_like_a_shell_line() {
        let mut line = Line::new("hello world");
        assert!(matches!(edit_key(&mut line, ctrl('w')), Edit::Changed));
        assert_eq!(line.text(), "hello ");
        edit_key(&mut line, key(KeyCode::Home));
        edit_key(&mut line, key(KeyCode::Delete));
        assert_eq!(line.text(), "ello ");
        edit_key(&mut line, ctrl('e'));
        edit_key(&mut line, key(KeyCode::Char('!')));
        assert_eq!(line.text(), "ello !");
        edit_key(&mut line, ctrl('a'));
        edit_key(&mut line, ctrl('k'));
        assert_eq!(line.text(), "");
        let mut line = Line::new("one two");
        edit_key(&mut line, ctrl('u'));
        assert_eq!((line.text().as_str(), line.cursor), ("", 0));
        // Backspace and the arrows stay inside the text.
        edit_key(&mut line, key(KeyCode::Backspace));
        edit_key(&mut line, key(KeyCode::Left));
        edit_key(&mut line, key(KeyCode::Delete));
        assert_eq!(line.cursor, 0);
        let mut line = Line::new("one two three");
        edit_key(
            &mut line,
            KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL),
        );
        assert_eq!(line.cursor, 8);
        edit_key(
            &mut line,
            KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL),
        );
        assert_eq!(line.cursor, 4);
        edit_key(
            &mut line,
            KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL),
        );
        assert_eq!(line.cursor, 8);
    }

    #[test]
    fn editing_keeps_whole_characters_and_a_bound() {
        let mut line = Line::new("a\u{30c7}b");
        line.left();
        line.backspace();
        assert_eq!(line.text(), "ab");
        let mut line = Line::new("");
        line.insert_str(&"x".repeat(5 * MAX_CHARS));
        assert_eq!(line.chars.len(), MAX_CHARS);
        // A paste stops at its first line break and drops control codes.
        let mut line = Line::new("");
        line.insert_str("one\ntwo");
        assert_eq!(line.text(), "one");
        let mut line = Line::new("");
        line.insert_str("a\u{1b}b\tc");
        assert_eq!(line.text(), "abc");
        // Control and Alt keys are not typed.
        let mut line = Line::new("");
        edit_key(&mut line, ctrl('x'));
        edit_key(
            &mut line,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT),
        );
        assert_eq!(line.text(), "");
        // AltGr arrives as Ctrl+Alt and is typed.
        edit_key(
            &mut line,
            KeyEvent::new(
                KeyCode::Char('@'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
        );
        assert_eq!(line.text(), "@");
    }

    #[test]
    fn esc_and_ctrl_c_cancel_and_enter_asks_the_validator() {
        let strict = |s: &str| {
            if s.is_empty() {
                Err("Give a name.".to_owned())
            } else {
                Ok(())
            }
        };
        let mut p = TextPrompt {
            label: "Name:",
            line: Line::new(""),
            hint: None,
            validate: &strict,
            error: None,
        };
        assert!(matches!(p.key(key(KeyCode::Esc)), Step::Cancel));
        assert!(matches!(p.key(ctrl('c')), Step::Cancel));
        assert!(matches!(p.key(key(KeyCode::Enter)), Step::Continue));
        assert_eq!(p.error.as_deref(), Some("Give a name."));
        // Typing clears the message; then Enter is accepted.
        assert!(matches!(p.key(key(KeyCode::Char('a'))), Step::Continue));
        assert!(p.error.is_none());
        assert!(matches!(p.key(key(KeyCode::Enter)), Step::Submit));
    }

    #[test]
    fn a_choice_moves_within_its_options() {
        let mut p = SelectPrompt {
            label: "Example:",
            options: &["one", "two", "three"],
            index: 0,
        };
        p.key(key(KeyCode::Up));
        assert_eq!(p.index, 0);
        p.key(key(KeyCode::Down));
        p.key(key(KeyCode::Char('j')));
        p.key(key(KeyCode::Down));
        assert_eq!(p.index, 2);
        p.key(key(KeyCode::Char('1')));
        assert_eq!(p.index, 0);
        p.key(key(KeyCode::Char('9')));
        assert_eq!(p.index, 0);
        p.key(key(KeyCode::End));
        assert_eq!(p.index, 2);
        let f = p.frame(80);
        assert_eq!(plain(&f.lines[3]), "> three");
        assert_eq!(plain(&f.lines[1]), "  one");
        assert_eq!(plain(&p.done(80).lines[0]), "? Example: three");
        assert!(matches!(p.key(key(KeyCode::Enter)), Step::Submit));
        assert!(matches!(p.key(key(KeyCode::Esc)), Step::Cancel));
    }

    #[test]
    fn a_question_takes_yes_no_and_the_default() {
        let mut p = ConfirmPrompt {
            label: "Create it?",
            value: true,
        };
        assert_eq!(plain(&p.frame(80).lines[0]), "? Create it? (Y/n)");
        assert!(matches!(p.key(key(KeyCode::Char('x'))), Step::Continue));
        assert!(matches!(p.key(key(KeyCode::Char('n'))), Step::Submit));
        assert!(!p.value);
        assert_eq!(plain(&p.frame(80).lines[0]), "? Create it? (y/N)");
        assert_eq!(plain(&p.done(80).lines[0]), "? Create it? No");
        assert!(matches!(p.key(key(KeyCode::Char('Y'))), Step::Submit));
        assert!(p.value);
        assert!(matches!(p.key(ctrl('c')), Step::Cancel));
    }

    #[test]
    fn long_choices_are_cut_with_dots() {
        assert_eq!(clip("abcdefgh", 6, true), "abc...");
        assert_eq!(clip("abcdefgh", 6, false), "abcdef");
        assert_eq!(clip("abc", 6, true), "abc");
        assert_eq!(text_width(&clip("\u{30c7}\u{30c3}\u{30ad}", 5, false)), 4);
    }
}
