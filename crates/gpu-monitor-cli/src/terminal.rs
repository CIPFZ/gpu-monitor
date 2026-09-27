//! Terminal ownership and restoration, including early errors and unwinding.

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, stdout, Stdout};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

pub struct TerminalSession {
    pub terminal: Tui,
    _restore: RestoreTerminal,
}

struct RestoreTerminal;

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = restore();
    }
}

pub fn init() -> io::Result<TerminalSession> {
    enable_raw_mode()?;
    let guard = RestoreTerminal;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;
    Ok(TerminalSession {
        terminal,
        _restore: guard,
    })
}

pub fn restore() -> io::Result<()> {
    // Always attempt both operations, even when one fails.
    let raw_result = disable_raw_mode();
    let screen_result = execute!(stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
    raw_result.and(screen_result)
}
