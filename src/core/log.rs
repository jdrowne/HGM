use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::Local;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warning => "WARNING",
            Level::Error => "ERROR",
        }
    }
}

#[derive(Clone, Default)]
pub struct Log {
    sink: Option<Arc<Mutex<File>>>,
}

impl Log {
    pub fn to_file(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Log {
            sink: Some(Arc::new(Mutex::new(file))),
        })
    }

    pub fn discard() -> Self {
        Log { sink: None }
    }

    pub fn line(level: Level, message: &str) -> String {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S,%3f");
        let thread = std::thread::current();
        let name = thread
            .name()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{:?}", thread.id()));
        format!("{now} - {name} - {} - {message}\n", level.name())
    }

    pub fn log(&self, level: Level, message: impl Display) {
        if let Some(sink) = &self.sink {
            let line = Self::line(level, &message.to_string());
            let mut file = sink
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }

    pub fn info(&self, message: impl Display) {
        self.log(Level::Info, message);
    }

    pub fn warning(&self, message: impl Display) {
        self.log(Level::Warning, message);
    }

    pub fn error(&self, message: impl Display) {
        self.log(Level::Error, message);
    }
}
