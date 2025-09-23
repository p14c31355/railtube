use crate::errors::{AppError, CommandError};
use reqwest::blocking::Client;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::process::Command;

// Function to log messages to a file
pub fn log_message(message: &str) -> Result<(), std::io::Error> {
    const LOG_FILE: &str = "railtube.log";
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)?;
    writeln!(file, "{}", message)?;
    Ok(())
}

// Helper function to log messages and handle errors
pub fn log_or_eprint(message: &str, error_message: &str) {
    if let Err(e) = log_message(message) {
        eprintln!("{}: {}", error_message, e);
    }
}

pub fn run_command(cmd: &str, args: &[&str]) -> Result<(), CommandError> {
    let command_str = format!("{} {}", cmd, args.join(" "));
    log_or_eprint(
        &format!("Executing: {}", command_str),
        "Failed to log message",
    );
    println!("Executing: {}", command_str);

    let mut command = Command::new(cmd);
    command.args(args);

    let output = command.output().map_err(|e| {
        let stderr_msg = format!("Error executing command '{}': {}", command_str, e);
        log_or_eprint(&stderr_msg, "Failed to log error message");
        CommandError {
            command: cmd.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            exit_code: None,
            stdout: String::new(),
            stderr: stderr_msg,
        }
    })?;

    let exit_code = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Log stdout and stderr regardless of success
    if !stdout.is_empty() {
        log_or_eprint(&format!("Stdout:\n{}", stdout), "Failed to log stdout");
    }
    if !stderr.is_empty() {
        log_or_eprint(&format!("Stderr:\n{}", stderr), "Failed to log stderr");
    }

    if !output.status.success() {
        let error_msg = format!(
            "Command failed with exit code {:?}: {}",
            exit_code, command_str
        );
        log_or_eprint(&error_msg, "Failed to log error message");
        return Err(CommandError {
            command: cmd.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            exit_code,
            stdout,
            stderr,
        });
    }
    Ok(())
}

pub fn fetch_toml_content(source: &str) -> Result<String, AppError> {
    if source.starts_with("http://") || source.starts_with("https://") {
        let client = Client::new();
        let mut response = client.get(source).send()?;
        if !response.status().is_success() {
            return Err(AppError::Other(
                format!("Failed to fetch URL: {}", response.status()).into(),
            ));
        }
        let mut content = String::new();
        response.read_to_string(&mut content)?;
        Ok(content)
    } else {
        std::fs::read_to_string(source).map_err(AppError::Io)
    }
}

pub fn confirm_installation(prompt: &str) -> Result<bool, AppError> {
    print!("{} (y/N): ", prompt);
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}
