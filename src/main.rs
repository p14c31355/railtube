use clap::{Parser, Subcommand};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::{collections::HashMap, fs, io::Read, process::Command};
use tempfile::tempdir;

// Custom error type for command execution
#[derive(Debug)]
struct CommandError {
    command: String,
    args: Vec<String>,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
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

#[derive(Debug, Deserialize)]
struct Config {
    system: Option<SystemSection>,
    apt: Option<Section>,
    snap: Option<Section>,
    flatpak: Option<Section>,
    cargo: Option<Section>,
    deb: Option<DebSection>,
    scripts: Option<ScriptsSection>,
}

#[derive(Debug, Deserialize)]
struct SystemSection {
    #[serde(default)] // Default to false if not present
    update: bool,
}

#[derive(Debug, Deserialize)]
struct Section {
    #[serde(default)] // Default to empty Vec if not present
    list: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DebSection {
    #[serde(default)] // Default to empty Vec if not present
    urls: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ScriptsSection {
    #[serde(flatten)]
    commands: HashMap<String, String>,
}

/// Railtube: Declarative OS Package Management
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Apply configurations from a TOML manifest
    Apply {
        /// The source of the TOML configuration file (local path or URL).
        #[arg(short, long)]
        source: String,
    },
    /// Run scripts defined in the TOML manifest
    Run {
        /// The source of the TOML configuration file (local path or URL).
        #[arg(short, long)]
        source: String,
        /// The name of the script to run from the [scripts] section.
        script_name: String,
    },
}

fn run_command(cmd: &str, args: &[&str]) -> Result<(), CommandError> {
    println!("> {} {}", cmd, args.join(" "));
    let mut command = Command::new(cmd);
    command.args(args);

    let output = command.output().map_err(|e| CommandError {
        command: cmd.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        exit_code: None,
        stdout: String::new(),
        stderr: e.to_string(),
    })?;

    let exit_code = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
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

fn fetch_toml_content(source: &str) -> Result<String, Box<dyn std::error::Error>> {
    if source.starts_with("http://") || source.starts_with("https://") {
        let client = Client::new(); // Client created here, outside the loop
        let mut response = client.get(source).send()?;
        if !response.status().is_success() {
            return Err(format!("Failed to fetch URL: {}", response.status()).into());
        }
        let mut content = String::new();
        response.read_to_string(&mut content)?;
        Ok(content)
    } else {
        fs::read_to_string(source).map_err(|e| e.into())
    }
}

fn apply_config(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    // Handle system updates
    if let Some(sys) = &config.system {
        if sys.update {
            run_command("sudo", &["apt", "update"])?;
        }
    }

    // Execute APT commands
    if let Some(apt) = &config.apt {
        for pkg in &apt.list {
            run_command("sudo", &["apt", "install", "-y", pkg])?;
        }
    }

    // Execute Snap commands
    if let Some(snap) = &config.snap {
        for pkg in &snap.list {
            run_command("sudo", &["snap", "install", pkg])?;
        }
    }

    // Execute Flatpak commands
    if let Some(flatpak) = &config.flatpak {
        for pkg in &flatpak.list {
            run_command("flatpak", &["install", "-y", pkg])?;
        }
    }

    // Execute Cargo install commands
    if let Some(cargo) = &config.cargo {
        for pkg in &cargo.list {
            run_command("cargo", &["install", pkg])?;
        }
    }

    // Handle .deb files
    if let Some(deb) = &config.deb {
        let temp_dir = tempdir()?;
        let client = Client::new(); // Client created here, outside the loop
        for url in &deb.urls {
            let filename = url.split('/').next_back().unwrap_or("package.deb");
            let temp_path = temp_dir.path().join(filename);

            println!("Downloading {} to {}", url, temp_path.display());
            let mut response = client.get(url).send()?;
            if !response.status().is_success() {
                return Err(format!("Failed to download {}: {}", url, response.status()).into());
            }
            let mut file = fs::File::create(&temp_path)?;
            response.copy_to(&mut file)?;

            println!("Installing {}...", temp_path.display());
            run_command("sudo", &["dpkg", "-i", temp_path.to_str().unwrap()])?;
            run_command("sudo", &["apt", "--fix-broken", "install", "-y"])?;
        }
    }

    Ok(())
}

fn run_scripts(
    config: &Config,
    script_name: &str,
    is_remote_source: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(scripts) = &config.scripts {
        if let Some(command_to_run) = scripts.commands.get(script_name) {
            println!("Running script '{}': {}", script_name, command_to_run);

            if is_remote_source {
                println!("WARNING: Executing script from a remote source.");
                // Prompt for confirmation
                print!("Do you want to proceed? (y/N): ");
                std::io::Write::flush(&mut std::io::stdout())?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Script execution aborted by user.");
                    return Ok(()); // User chose not to proceed
                }
            }

            // Execute the command string using a shell for flexibility
            run_command("sh", &["-c", command_to_run])?;
        } else {
            eprintln!("Script '{}' not found in [scripts] section.", script_name);
            return Err(format!("Script '{}' not found.", script_name).into());
        }
    } else {
        eprintln!("No [scripts] section found in the TOML configuration.");
        return Err("No [scripts] section found.".into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Fetch and parse TOML configuration
    let config: Config = match args.command {
        Commands::Apply { ref source } => {
            let toml_str = fetch_toml_content(source)?;
            toml::from_str(&toml_str)
                .map_err(|e: toml::de::Error| Box::new(e) as Box<dyn std::error::Error>)?
        }
        Commands::Run {
            ref source,
            ref script_name,
        } => {
            let toml_str = fetch_toml_content(source)?;
            toml::from_str(&toml_str)
                .map_err(|e: toml::de::Error| Box::new(e) as Box<dyn std::error::Error>)?
        }
    };

    // Execute the appropriate command
    match args.command {
        Commands::Apply { ref source } => {
            apply_config(&config)?;
        }
        Commands::Run {
            ref source,
            ref script_name,
        } => {
            let is_remote = source.starts_with("http://") || source.starts_with("https://");
            run_scripts(&config, script_name, is_remote)?;
        }
    };

    Ok(())
}
