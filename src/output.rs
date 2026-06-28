use std::{
    env,
    io::{self, IsTerminal, Write},
};

use anstream::{AutoStream, ColorChoice};
use anstyle::{AnsiColor, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Pretty,
    Plain,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandScope {
    Init,
    Setup,
    Doctor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Step,
    Info,
    Ok,
    Warn,
    Error,
}

pub struct OutputContext {
    mode: OutputMode,
    scope: CommandScope,
    color_enabled: bool,
}

impl OutputContext {
    pub fn new(mode: OutputMode, scope: CommandScope) -> Self {
        let color_enabled = mode == OutputMode::Pretty
            && io::stdout().is_terminal()
            && env::var_os("NO_COLOR").is_none();
        Self {
            mode,
            scope,
            color_enabled,
        }
    }

    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    pub fn step(&self, message: impl AsRef<str>) {
        self.level(Level::Step, message.as_ref());
    }

    pub fn info(&self, message: impl AsRef<str>) {
        self.level(Level::Info, message.as_ref());
    }

    pub fn ok(&self, message: impl AsRef<str>) {
        self.level(Level::Ok, message.as_ref());
    }

    pub fn warn(&self, message: impl AsRef<str>) {
        self.level(Level::Warn, message.as_ref());
    }

    pub fn error(&self, message: impl AsRef<str>) {
        self.level(Level::Error, message.as_ref());
    }

    pub fn status(&self, status: &str) {
        match self.mode {
            OutputMode::Pretty => {
                self.write_stdout_line(&format!("{} status {}", self.prefix(), status))
            }
            OutputMode::Plain => {
                self.write_stdout_line(&format!("{} status={status}", self.prefix()))
            }
            OutputMode::Json => {}
        }
    }

    pub fn summary(&self, message: &str) {
        match self.mode {
            OutputMode::Pretty => self.ok(message),
            OutputMode::Plain => {
                self.write_stdout_line(&format!("{} summary={message}", self.prefix()))
            }
            OutputMode::Json => {}
        }
    }

    pub fn kv(&self, key: &str, value: &str) {
        match self.mode {
            OutputMode::Pretty => self.info(format!("{key:<13} {value}")),
            OutputMode::Plain => {
                self.write_stdout_line(&format!("{} {key}={value}", self.prefix()))
            }
            OutputMode::Json => {}
        }
    }

    pub fn blank(&self) {
        if self.mode == OutputMode::Pretty {
            self.write_stdout_line("");
        }
    }

    fn level(&self, level: Level, message: &str) {
        match self.mode {
            OutputMode::Pretty => self.write_stdout_line(&format!(
                "{} {} {message}",
                self.prefix(),
                self.render_level(level)
            )),
            OutputMode::Plain => self.write_stdout_line(&format!(
                "{} {}={message}",
                self.prefix(),
                level.plain_key()
            )),
            OutputMode::Json => {}
        }
    }

    fn prefix(&self) -> String {
        format!("[gdsuite:{}]", self.scope.as_str())
    }

    fn render_level(&self, level: Level) -> String {
        let label = format!("{:<5}", level.pretty_label());
        if !self.color_enabled {
            return label;
        }

        let style = level.style();
        format!("{}{}{}", style.render(), label, style.render_reset())
    }

    fn write_stdout_line(&self, line: &str) {
        let choice = if self.color_enabled {
            ColorChoice::Auto
        } else {
            ColorChoice::Never
        };
        let mut stream = AutoStream::new(io::stdout(), choice);
        let _ = writeln!(stream, "{line}");
    }
}

impl CommandScope {
    fn as_str(self) -> &'static str {
        match self {
            CommandScope::Init => "init",
            CommandScope::Setup => "setup",
            CommandScope::Doctor => "doctor",
        }
    }
}

impl Level {
    fn pretty_label(self) -> &'static str {
        match self {
            Level::Step => "step",
            Level::Info => "info",
            Level::Ok => "ok",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }

    fn plain_key(self) -> &'static str {
        match self {
            Level::Step => "step",
            Level::Info => "info",
            Level::Ok => "ok",
            Level::Warn => "warning",
            Level::Error => "error",
        }
    }

    fn style(self) -> Style {
        match self {
            Level::Step => Style::new().fg_color(Some(AnsiColor::Blue.into())).bold(),
            Level::Info => Style::new().fg_color(Some(AnsiColor::Cyan.into())),
            Level::Ok => Style::new().fg_color(Some(AnsiColor::Green.into())).bold(),
            Level::Warn => Style::new().fg_color(Some(AnsiColor::Yellow.into())).bold(),
            Level::Error => Style::new().fg_color(Some(AnsiColor::Red.into())).bold(),
        }
    }
}
