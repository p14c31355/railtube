use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::{fs, process::Command, io::{Read, Write}, path::PathBuf, collections::HashMap, fs::OpenOptions};
use reqwest::blocking::Client;
use tempfile::tempdir;
use rayon::prelude::*; // Import rayon for parallel processing

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
        write!(f, "Command failed: {} {}\n", self.command, self.args.join(" "))?;
        if let Some(code) = self.exit_code {
            write!(f, "Exit code: {}\n", code)?;
        }
        if !self.stdout.is_empty() {
            write!(f, "Stdout: {}\n", self.stdout)?;
        }
        if !self.stderr.is_empty() {
            write!(f, "Stderr: {}\n", self.stderr)?;
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

// Function to log messages to a file
fn log_message(message: &str) -> Result<(), std::io::Error> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("railtube.log")?;
    writeln!(file, "{}", message)?;
    Ok(())
}

fn run_command(cmd: &str, args: &[&str]) -> Result<(), CommandError> {
    let log_message_str = format!("> {} {}", cmd, args.join(" "));
    log_message(&log_message_str).unwrap_or_else(|e| eprintln!("Failed to log message: {}", e));
    println!("{}", log_message_str);

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
        let error_msg = format!(
            "Command failed: {} {}\nExit code: {:?}\nStdout: {}\nStderr: {}",
            cmd,
            args.join(" "),
            exit_code,
            stdout,
            stderr
        );
        log_message(&error_msg).unwrap_or_else(|e| eprintln!("Failed to log error message: {}", e));
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

// Helper to check if a Cargo package is installed
fn is_cargo_package_installed(pkg_name: &str) -> Result<bool, String> { // Changed return type to String for Send
    let output = Command::new("cargo")
        .arg("install")
        .arg("--list")
        .output();
    
    match output {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("Warning: Failed to list installed cargo packages. Assuming '{}' is not installed.", pkg_name);
                return Ok(false);
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.lines().any(|line| line.starts_with(&format!("{} ", pkg_name))))
        }
        Err(e) => {
            eprintln!("Warning: Error executing 'cargo install --list': {}. Assuming '{}' is not installed.", e, pkg_name);
            Ok(false)
        }
    }
}

// Helper to check if a Snap package is installed
fn is_snap_package_installed(pkg_name: &str) -> Result<bool, String> { // Changed return type to String for Send
    // Snap package names can sometimes have arguments like "code --classic".
    // We need to extract the base package name.
    let base_pkg_name = pkg_name.split_whitespace().next().unwrap_or(pkg_name);

    let output = Command::new("snap")
        .arg("list")
        .output();
    
    match output {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("Warning: Failed to list installed snap packages. Assuming '{}' is not installed.", base_pkg_name);
                return Ok(false);
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            // The output format is typically "Name Version Rev Tracking Publisher Notes"
            // We need to check if the package name exists in the first column.
            Ok(stdout.lines().any(|line| line.split_whitespace().next() == Some(base_pkg_name)))
        }
        Err(e) => {
            eprintln!("Warning: Error executing 'snap list': {}. Assuming '{}' is not installed.", e, base_pkg_name);
            Ok(false)
        }
    }
}

// Helper to check if a Flatpak package is installed
fn is_flatpak_package_installed(pkg_name: &str) -> Result<bool, String> { // Changed return type to String for Send
    let output = Command::new("flatpak")
        .arg("list")
        .output();
    
    match output {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("Warning: Failed to list installed flatpak packages. Assuming '{}' is not installed.", pkg_name);
                return Ok(false);
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            // The output format is typically "Name Version ApplicationID Runtime Origin Installation"
            // We need to check if the ApplicationID exists in the third column.
            Ok(stdout.lines().any(|line| line.split_whitespace().nth(2) == Some(pkg_name)))
        }
        Err(e) => {
            eprintln!("Warning: Error executing 'flatpak list': {}. Assuming '{}' is not installed.", e, pkg_name);
            Ok(false)
        }
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
        // APT package installation is not easily parallelized due to sudo and potential dependencies.
        // We process them sequentially for now.
        for pkg in &apt.list {
            // APT version pinning is complex and requires specific syntax like "package=version"
            // For now, we install without version pinning.
            run_command("sudo", &["apt", "install", "-y", pkg])?;
        }
    }

    // Execute Snap commands in parallel
    if let Some(snap) = &config.snap {
        snap.list.iter().for_each(|pkg| {
            let pkg_name = pkg.split_whitespace().next().unwrap_or(pkg); // Get base name for check
            if !is_snap_package_installed(pkg_name).unwrap_or(false) { // Handle potential errors from check
                if let Err(e) = run_command("sudo", &["snap", "install", pkg]) {
                    eprintln!("Error installing snap package '{}': {}", pkg_name, e);
                }
            } else {
                println!("Snap package '{}' already installed, skipping.", pkg_name);
            }
        });
    }

    // Execute Flatpak commands in parallel
    if let Some(flatpak) = &config.flatpak {
        flatpak.list.iter().for_each(|pkg| {
            if !is_flatpak_package_installed(pkg).unwrap_or(false) { // Handle potential errors from check
                if let Err(e) = run_command("flatpak", &["install", "-y", pkg]) {
                    eprintln!("Error installing flatpak package '{}': {}", pkg, e);
                }
            } else {
                println!("Flatpak package '{}' already installed, skipping.", pkg);
            }
        });
    }

    // Execute Cargo install commands in parallel
    if let Some(cargo) = &config.cargo {
        cargo.list.iter().for_each(|pkg| {
            if !is_cargo_package_installed(pkg).unwrap_or(false) { // Handle potential errors from check
                if let Err(e) = run_command("cargo", &["install", pkg]) {
                    eprintln!("Error installing cargo package '{}': {}", pkg, e);
                }
            } else {
                println!("Cargo package '{}' already installed, skipping.", pkg);
            }
        });
    }

    // Handle .deb files
    if let Some(deb) = &config.deb {
        let temp_dir = tempdir()?;
        let client = Client::new(); // Client created here, outside the loop
        for url in &deb.urls {
            let filename = url.split('/').last().unwrap_or("package.deb");
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

fn run_scripts(config: &Config, script_name: &str, is_remote_source: bool) -> Result<(), Box<dyn std::error::Error>> {
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
            toml::from_str(&toml_str).map_err(|e: toml::de::Error| Box::new(e) as Box<dyn std::error::Error>)?
        }
        Commands::Run { ref source, ref script_name } => {
            let toml_str = fetch_toml_content(source)?;
            toml::from_str(&toml_str).map_err(|e: toml::de::Error| Box::new(e) as Box<dyn std::error::Error>)?
        }
    };

    // Execute the appropriate command
    match args.command {
        Commands::Apply { ref source } => {
            apply_config(&config)?;
        }
        Commands::Run { ref source, ref script_name } => {
            let is_remote = source.starts_with("http://") || source.starts_with("https://");
            run_scripts(&config, script_name, is_remote)?;
        }
    };

    Ok(())
}
