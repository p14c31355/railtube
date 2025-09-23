use thiserror::Error;

#[derive(Debug)]
pub struct CommandError {
    pub command: String,
    pub args: Vec<String>,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Command failed: {} {}",
            self.command,
            self.args.join(" ")
        )?;
        if let Some(code) = self.exit_code {
            writeln!(f, "Exit code: {}", code)?;
        }
        if !self.stdout.is_empty() {
            writeln!(f, "Stdout: {}", self.stdout)?;
        }
        if !self.stderr.is_empty() {
            writeln!(f, "Stderr: {}", self.stderr)?;
        }
        Ok(())
    }
}

impl std::error::Error for CommandError {}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Command Error: {0}")]
    Command(#[from] CommandError),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Fetch Error: {0}")]
    Fetch(#[from] reqwest::Error),
    #[error("TOML Deserialization Error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("Other Error: {0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}
