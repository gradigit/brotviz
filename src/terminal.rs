use anyhow::Context;
use crossterm::{
    cursor,
    terminal::{self, ClearType},
    ExecutableCommand,
};
use std::io::{stdout, Stdout, Write};

pub struct TerminalGuard {
    _private: (),
}

impl TerminalGuard {
    pub fn new() -> anyhow::Result<Self> {
        terminal::enable_raw_mode().context("enable raw mode")?;
        // Create the guard immediately so Drop will disable raw mode if
        // any subsequent setup step fails.
        let guard = Self { _private: () };

        let mut out = stdout();
        out.execute(terminal::EnterAlternateScreen)
            .context("enter alternate screen")?;
        out.execute(terminal::Clear(ClearType::All))
            .context("clear screen")?;
        out.execute(cursor::Hide).context("hide cursor")?;

        Ok(guard)
    }

    pub fn stdout() -> Stdout {
        stdout()
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let mut out = stdout();
        // Best-effort: undo modes we may have enabled during rendering (sync output, autowrap, colors).
        let _ = out.write_all(b"\x1b[?2026l\x1b[?7h\x1b[0m");
        let _ = out.flush();
        let _ = out.execute(cursor::Show);
        let _ = out.execute(terminal::LeaveAlternateScreen);
    }
}
