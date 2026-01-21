// ABOUTME: Output formatting for CLI feedback.
// ABOUTME: Supports normal, quiet (CI), and JSON output modes.

use serde::Serialize;
use std::time::Instant;

/// Output mode for CLI feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Human-friendly output with progress messages
    Normal,
    /// Minimal output for CI (only final result)
    Quiet,
    /// JSON lines for scripting
    Json,
}

/// Handles CLI output based on the configured mode.
pub struct Output {
    mode: OutputMode,
    start_time: Option<Instant>,
}

impl Output {
    pub fn new(mode: OutputMode) -> Self {
        Self {
            mode,
            start_time: None,
        }
    }

    /// Start timing an operation.
    pub fn start_timer(&mut self) {
        self.start_time = Some(Instant::now());
    }

    /// Get elapsed time since timer started.
    pub fn elapsed_secs(&self) -> f64 {
        self.start_time
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }

    /// Print a progress message (suppressed in quiet/json mode).
    pub fn progress(&self, message: &str) {
        if self.mode == OutputMode::Normal {
            println!("{message}");
        }
    }

    /// Print a success message with optional timing.
    pub fn success(&self, message: &str) {
        match self.mode {
            OutputMode::Normal => {
                let elapsed = self.elapsed_secs();
                if elapsed > 0.0 {
                    println!("{message} ({:.1}s)", elapsed);
                } else {
                    println!("{message}");
                }
            }
            OutputMode::Quiet => {
                // Print only the essential result
                println!("{message}");
            }
            OutputMode::Json => {
                let event = JsonEvent {
                    event: "success",
                    message,
                    duration_secs: if self.start_time.is_some() {
                        Some(self.elapsed_secs())
                    } else {
                        None
                    },
                };
                if let Ok(json) = serde_json::to_string(&event) {
                    println!("{json}");
                }
            }
        }
    }

    /// Print an error message.
    pub fn error(&self, message: &str) {
        match self.mode {
            OutputMode::Normal | OutputMode::Quiet => {
                eprintln!("Error: {message}");
            }
            OutputMode::Json => {
                let event = JsonEvent {
                    event: "error",
                    message,
                    duration_secs: if self.start_time.is_some() {
                        Some(self.elapsed_secs())
                    } else {
                        None
                    },
                };
                if let Ok(json) = serde_json::to_string(&event) {
                    eprintln!("{json}");
                }
            }
        }
    }
}

#[derive(Serialize)]
struct JsonEvent<'a> {
    event: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_secs: Option<f64>,
}
