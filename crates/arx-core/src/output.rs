use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Json,
}

impl OutputMode {
    pub fn detect() -> Self {
        if let Ok(val) = std::env::var("ARX_OUTPUT") {
            if val == "json" {
                return Self::Json;
            }
        }
        if is_terminal::is_terminal(std::io::stdout()) {
            Self::Human
        } else {
            Self::Json
        }
    }
}

pub fn print_result<T: Serialize + std::fmt::Display>(mode: OutputMode, value: &T) {
    match mode {
        OutputMode::Human => println!("{value}"),
        OutputMode::Json => {
            if let Ok(json) = serde_json::to_string(value) {
                println!("{json}");
            }
        }
    }
}

pub fn print_error(mode: OutputMode, error: &crate::error::Error) {
    match mode {
        OutputMode::Human => eprintln!("error: {error}"),
        OutputMode::Json => {
            let val = serde_json::json!({
                "error": error.to_string(),
            });
            eprintln!("{}", serde_json::to_string(&val).unwrap_or_default());
        }
    }
}
