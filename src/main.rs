use clap::{Parser, Subcommand};
use rayon::prelude::*;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize}; // Added Serialize
use std::{
    collections::HashMap,
    fs,
    fs::OpenOptions,
    io::{Read, Write},
    process::Command,
};
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

#[derive(Debug, Deserialize, Serialize)] // Added Serialize
struct Config {
    system: Option<SystemSection>,
    apt: Option<Section>,
    snap: Option<Section>,
    flatpak: Option<Section>,
    cargo: Option<Section>,
    deb: Option<DebSection>,
    scripts: Option<ScriptsSection>,
}

#[derive(Debug, Deserialize, Serialize)] // Added Serialize
struct SystemSection {
    #[serde(default)] // Default to false if not present
    update: bool,
}

#[derive(Debug, Deserialize, Serialize)] // Added Serialize
struct Section {
    #[serde(default)] // Default to empty Vec if not present
    list: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)] // Added Serialize
struct DebSection {
    #[serde(default)] // Default to empty Vec if not present
    urls: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)] // Added Serialize
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
        /// Perform a dry run, showing what would be installed without actually installing anything.
        #[arg(long, default_value = "false")]
        dry_run: bool,
    },
    /// Run scripts defined in the TOML manifest
    Run {
        /// The source of the TOML configuration file (local path or URL).
        #[arg(short, long)]
        source: String,
        /// The name of the script to run from the [scripts] section.
        script_name: String,
    },
    /// Export the current environment to a TOML manifest
    Export {
        /// The output file path for the generated TOML manifest.
        #[arg(short, long, default_value = "exported-env.toml")]
        output: String,
    },
}

// Function to log messages to a file
fn log_message(message: &str) -> Result<(), std::io::Error> {
    const LOG_FILE: &str = "railtube.log";
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)?;
    writeln!(file, "{}", message)?;
    Ok(())
}

// Helper function to log messages and handle errors
fn log_or_eprint(message: &str, error_message: &str) {
    if let Err(e) = log_message(message) {
        eprintln!("{}: {}", error_message, e);
    }
}

fn run_command(cmd: &str, args: &[&str]) -> Result<(), CommandError> {
    let command_str = format!("{} {}", cmd, args.join(" "));
    log_or_eprint(&format!("Executing: {}", command_str), "Failed to log message");
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
fn is_cargo_package_installed(pkg_name: &str) -> Result<bool, String> {
    // Changed return type to String for Send
    let output = Command::new("cargo").arg("install").arg("--list").output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("Warning: Failed to list installed cargo packages. Assuming '{}' is not installed.", pkg_name);
                return Ok(false);
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            // The output format is typically "package_name vX.Y.Z"
            // We need to check if the package name exists in the output.
            // A simple check for the package name followed by a space or newline should suffice.
            Ok(stdout
                .lines()
                .any(|line| line.starts_with(&format!("{} ", pkg_name))))
        }
        Err(e) => {
            eprintln!("Warning: Error executing 'cargo install --list': {}. Assuming '{}' is not installed.", e, pkg_name);
            Ok(false)
        }
    }
}

// Helper to check if a Snap package is installed
fn is_snap_package_installed(pkg_name: &str) -> Result<bool, String> {
    // Changed return type to String for Send
    // Snap package names can sometimes have arguments like "code --classic".
    // We need to extract the base package name.
    let base_pkg_name = pkg_name.split_whitespace().next().unwrap_or(pkg_name);

    let output = Command::new("snap").arg("list").output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("Warning: Failed to list installed snap packages. Assuming '{}' is not installed.", base_pkg_name);
                return Ok(false);
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            // The output format is typically "Name Version Rev Tracking Publisher Notes"
            // We need to check if the package name exists in the first column.
            Ok(stdout
                .lines()
                .any(|line| line.split_whitespace().next() == Some(base_pkg_name)))
        }
        Err(e) => {
            eprintln!(
                "Warning: Error executing 'snap list': {}. Assuming '{}' is not installed.",
                e, base_pkg_name
            );
            Ok(false)
        }
    }
}

// Helper to check if a Flatpak package is installed
fn is_flatpak_package_installed(pkg_name: &str) -> Result<bool, String> {
    // Changed return type to String for Send
    let output = Command::new("flatpak").arg("list").output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("Warning: Failed to list installed flatpak packages. Assuming '{}' is not installed.", pkg_name);
                return Ok(false);
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            // The output format is typically "Name Version ApplicationID Runtime Origin Installation"
            // We need to check if the ApplicationID exists in the third column.
            Ok(stdout
                .lines()
                .any(|line| line.split_whitespace().nth(2) == Some(pkg_name)))
        }
        Err(e) => {
            eprintln!(
                "Warning: Error executing 'flatpak list': {}. Assuming '{}' is not installed.",
                e, pkg_name
            );
            Ok(false)
        }
    }
}

// Helper to get installed APT packages
fn get_installed_apt_packages() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut packages = Vec::new();
    let output = Command::new("apt")
        .arg("list")
        .arg("--installed")
        .output()?;

    if !output.status.success() {
        return Err("Failed to list installed APT packages.".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Example line: "ii  package-name:amd64        1.2.3-1ubuntu1  amd64"
        // We want to extract "package-name"
        if line.starts_with("ii ") {
            if let Some(pkg_info) = line.split_whitespace().nth(1) {
                // Remove architecture suffix like ":amd64" if present
                if let Some(pkg_name) = pkg_info.split(':').next() {
                    packages.push(pkg_name.to_string());
                }
            }
        }
    }
    Ok(packages)
}

// Helper to get installed Cargo packages
fn get_installed_cargo_packages() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut packages = Vec::new();
    let output = Command::new("cargo")
        .arg("install")
        .arg("--list")
        .output()?;

    if !output.status.success() {
        return Err("Failed to list installed Cargo packages.".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Example line: "package-name v1.2.3"
        if let Some(pkg_name) = line.split_whitespace().next() {
            packages.push(pkg_name.to_string());
        }
    }
    Ok(packages)
}

// Helper to get installed Snap packages
fn get_installed_snap_packages() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut packages = Vec::new();
    let output = Command::new("snap").arg("list").output()?;

    if !output.status.success() {
        return Err("Failed to list installed Snap packages.".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Example line: "Name Version Rev Tracking Publisher Notes"
        if let Some(pkg_name) = line.split_whitespace().next() {
            packages.push(pkg_name.to_string());
        }
    }
    Ok(packages)
}

// Helper to get installed Flatpak packages
fn get_installed_flatpak_packages() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut packages = Vec::new();
    let output = Command::new("flatpak").arg("list").output()?;

    if !output.status.success() {
        return Err("Failed to list installed Flatpak packages.".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Example line: "Name Version ApplicationID Runtime Origin Installation"
        // We want the ApplicationID (3rd column)
        if let Some(app_id) = line.split_whitespace().nth(2) {
            packages.push(app_id.to_string());
        }
    }
    Ok(packages)
}

fn apply_config(config: &Config, dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
    // Handle system updates
    if let Some(sys) = &config.system {
        if sys.update {
            if dry_run {
                println!("Would run: sudo apt update");
            } else {
                run_command("sudo", &["apt", "update"])?;
            }
        }
    }

    // Execute APT commands
    if let Some(apt) = &config.apt {
        // APT package installation is not easily parallelized due to sudo and potential dependencies.
        // We process them sequentially for now.
        for pkg_spec in &apt.list {
            let mut parts = pkg_spec.splitn(2, '=');
            let pkg_name = parts.next().unwrap_or(pkg_spec);
            let version = parts.next(); // This will be Some("version") or None

            let mut apt_args = vec!["install", "-y"];
            if version.is_some() {
                apt_args.push(pkg_spec); // Use the full "package=version" string
            } else {
                apt_args.push(pkg_name);
            }

            // Log the specific action before running the command
            let action_desc = if version.is_some() {
                format!("Installing APT package '{}' with version", pkg_spec)
            } else {
                format!("Installing APT package '{}'", pkg_name)
            };
            log_or_eprint(&action_desc, "Failed to log message");
            println!("{}", action_desc);

            if dry_run {
                println!("Would run: sudo {}", apt_args.join(" "));
            } else {
                run_command("sudo", &apt_args)?;
            }
        }
    }

    // Execute Snap commands in parallel
    if let Some(snap) = &config.snap {
        snap.list.par_iter().for_each(|pkg| {
            let pkg_name = pkg.split_whitespace().next().unwrap_or(pkg); // Get base name for check
            if !is_snap_package_installed(pkg_name).unwrap_or(false) {
                // Handle potential errors from check
                let command_str = format!("sudo snap install {}", pkg);
                if dry_run {
                    println!("Would run: {}", command_str);
                } else if let Err(e) = run_command("sudo", &["snap", "install", pkg]) {
                    eprintln!("Error installing snap package '{}': {}", pkg_name, e);
                }
            } else {
                println!("Snap package '{}' already installed, skipping.", pkg_name);
            }
        });
    }

    // Execute Flatpak commands in parallel
    if let Some(flatpak) = &config.flatpak {
        flatpak.list.par_iter().for_each(|pkg| {
            if !is_flatpak_package_installed(pkg).unwrap_or(false) {
                // Handle potential errors from check
                let command_str = format!("flatpak install -y {}", pkg);
                if dry_run {
                    println!("Would run: {}", command_str);
                } else if let Err(e) = run_command("flatpak", &["install", "-y", pkg]) {
                    eprintln!("Error installing flatpak package '{}': {}", pkg, e);
                }
            } else {
                println!("Flatpak package '{}' already installed, skipping.", pkg);
            }
        });
    }

    // Execute Cargo install commands in parallel
    if let Some(cargo) = &config.cargo {
        cargo.list.par_iter().for_each(|pkg| {
            if !is_cargo_package_installed(pkg).unwrap_or(false) {
                // Handle potential errors from check
                let command_str = format!("cargo install {}", pkg);
                if dry_run {
                    println!("Would run: {}", command_str);
                } else if let Err(e) = run_command("cargo", &["install", pkg]) {
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
            if dry_run {
                println!("Would run: sudo dpkg -i {}", temp_path.display());
                println!("Would run: sudo apt --fix-broken install -y");
            } else {
                run_command(
                    "sudo",
                    &[
                        "dpkg",
                        "-i",
                        temp_path
                            .to_str()
                            .ok_or("Temporary path is not valid UTF-8")?,
                    ],
                )?;
                run_command("sudo", &["apt", "--fix-broken", "install", "-y"])?;
            }
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

// Function to export the current environment to a TOML manifest
fn export_current_environment() -> Result<Config, Box<dyn std::error::Error>> {
    let config = Config {
        system: Some(SystemSection { update: false }), // Default to false for export
        apt: Some(Section {
            list: get_installed_apt_packages()?,
        }),
        snap: Some(Section {
            list: get_installed_snap_packages()?,
        }),
        flatpak: Some(Section {
            list: get_installed_flatpak_packages()?,
        }),
        cargo: Some(Section {
            list: get_installed_cargo_packages()?,
        }),
        deb: None, // Deb files are usually downloaded from URLs, not installed system-wide in a way that's easily exportable
        scripts: None, // Scripts are defined, not exported from current state
    };

    // For simplicity, we'll assume no system updates are exported by default.
    // If a system update was performed, it might be hard to determine if it was a "default" update or a specific one.
    // We can refine this later if needed.

    Ok(config)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Fetch and parse TOML configuration
    let config: Config = match args.command {
        Commands::Apply {
            ref source,
            dry_run,
        } => {
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
        Commands::Export { ref output } => {
            let exported_config = export_current_environment()?;
            let toml_string = toml::to_string_pretty(&exported_config)?;
            let mut file = fs::File::create(output)?;
            file.write_all(toml_string.as_bytes())?;
            println!("Environment exported to {}", output);
            return Ok(()); // Exit after export
        }
    };

    // Execute the appropriate command
    match args.command {
        Commands::Apply {
            ref source,
            dry_run,
        } => {
            apply_config(&config, dry_run)?;
        }
        Commands::Run {
            ref source,
            ref script_name,
        } => {
            let is_remote = source.starts_with("http://") || source.starts_with("https://");
            run_scripts(&config, script_name, is_remote)?;
        }
        Commands::Export { .. } => {
            // This branch is now handled above.
        }
    };

    Ok(())
}
