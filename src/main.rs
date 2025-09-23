use clap::{Parser, Subcommand};
use rayon::prelude::*;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize}; // Added Serialize
use thiserror::Error;
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

#[derive(Error, Debug)]
enum AppError {
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
        /// Skip confirmation prompts for installations.
        #[arg(long, default_value = "false")]
        yes: bool,
        /// Apply configurations to specific sections only (e.g., cargo, apt).
        #[arg(long, value_delimiter = ',')] // Allow multiple comma-separated values
        only: Option<Vec<String>>,
    },
    /// Run scripts defined in the TOML manifest
    Run {
        /// The source of the TOML configuration file (local path or URL).
        #[arg(short, long)]
        source: String,
        /// The name of the script to run from the [scripts] section.
        script_name: String,
    },
    /// Run the doctor command to check installed packages against the TOML manifest.
    Doctor {
        /// The source of the TOML configuration file (local path or URL).
        #[arg(short, long)]
        source: String,
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

fn fetch_toml_content(source: &str) -> Result<String, AppError> {
    if source.starts_with("http://") || source.starts_with("https://") {
        let client = Client::new(); // Client created here, outside the loop
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
        fs::read_to_string(source).map_err(AppError::Io)
    }
}

// Helper to check if a Cargo package is installed
fn is_cargo_package_installed(pkg_name: &str) -> bool {
    let cargo_bin_path = match std::env::var("CARGO_HOME") {
        Ok(val) => std::path::PathBuf::from(val).join("bin"),
        Err(_) => {
            // Fallback if CARGO_HOME is not set or dirs::home_dir() fails.
            // This fallback is a bit simplistic and might not work on all systems.
            // A more robust solution would involve better error handling or a different crate.
            dirs::home_dir()
                .map(|home| home.join(".cargo").join("bin"))
                .unwrap_or_else(|| {
                    // If home_dir() also fails, we can't reliably find cargo bin.
                    // Log a warning and proceed to the fallback check.
                    eprintln!("Warning: Could not determine CARGO_HOME or home directory. Proceeding with 'cargo install --list' fallback.");
                    std::path::PathBuf::new() // Return an empty path, which will likely fail exists() check
                })
        }
    };

    // Check if the executable exists in the determined cargo bin path
    if !cargo_bin_path.as_os_str().is_empty() {
        // Ensure we have a valid path before checking existence
        let executable_path = cargo_bin_path.join(pkg_name);
        if executable_path.exists() {
            return true;
        }
    }

    // Fallback to `cargo install --list` if executable not found or cargo_home failed
    let output = Command::new("cargo").arg("install").arg("--list").output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("Warning: Failed to list installed cargo packages. Assuming '{}' is not installed.", pkg_name);
                return false;
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            // The output format is typically "package_name vX.Y.Z"
            // We need to check if the package name exists in the output.
            // A simple check for the package name followed by a space or newline should suffice.
            // Improved check using grep-like logic: filter lines starting with "pkg " and check for pkg_name.
            stdout
                .lines()
                .any(|line| line.trim_start().starts_with(&format!("{} v", pkg_name)))
        }
        Err(e) => {
            eprintln!("Warning: Error executing 'cargo install --list': {}. Assuming '{}' is not installed.", e, pkg_name);
            false
        }
    }
}

// Helper to check if a Snap package is installed
fn is_snap_package_installed(pkg_name: &str) -> bool {
    // Snap package names can sometimes have arguments like "code --classic".
    // We need to extract the base package name for the check.
    let base_pkg_name = pkg_name.split_whitespace().next().unwrap_or(pkg_name);

    // Use `snap info` which is more direct. It returns a success status if the package is installed.
    let output = Command::new("snap").arg("info").arg(base_pkg_name).output();

    match output {
        Ok(output) => output.status.success(),
        Err(e) => {
            eprintln!(
                "Warning: Error executing 'snap info': {}. Assuming '{}' is not installed.",
                e, base_pkg_name
            );
            false
        }
    }
}

// Helper to check if a Flatpak package is installed
fn is_flatpak_package_installed(pkg_name: &str) -> bool {
    // Use `flatpak info` which is more direct and reliable.
    // It returns a success status code if the package is installed.
    let output = Command::new("flatpak").arg("info").arg(pkg_name).output();

    match output {
        Ok(output) => output.status.success(),
        Err(e) => {
            eprintln!(
                "Warning: Error executing 'flatpak info': {}. Assuming '{}' is not installed.",
                e, pkg_name
            );
            false
        }
    }
}

// Helper to get installed APT packages
fn get_installed_apt_packages() -> Result<Vec<String>, AppError> {
    let output = Command::new("dpkg-query")
        .arg("-W")
        .arg("-f=${Package}\n")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Other(
            format!(
                "Failed to list installed APT packages with dpkg-query: {}",
                stderr
            )
            .into(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

// Helper to get the installed version of an APT package
fn get_installed_apt_version(pkg_name: &str) -> Result<Option<String>, AppError> {
    let output = Command::new("dpkg-query")
        .arg("-W")
        .arg("-f")
        .arg("${Version}")
        .arg(pkg_name)
        .output()?;

    if !output.status.success() {
        // Package not found or other error
        if String::from_utf8_lossy(&output.stderr).contains("no packages found") {
            return Ok(None); // Package not installed
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Other(
            format!(
                "Failed to query version for APT package '{}': {}",
                pkg_name, stderr
            )
            .into(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // The output might be enclosed in single quotes, e.g., '1.2.3'
    let version = stdout.trim().trim_matches('\'').to_string();
    Ok(Some(version))
}

// Helper to get installed Cargo packages
fn get_installed_cargo_packages() -> Result<Vec<String>, AppError> {
    let mut packages = Vec::new();
    let output = Command::new("cargo")
        .arg("install")
        .arg("--list")
        .output()?;

    if !output.status.success() {
        return Err(AppError::Other(
            "Failed to list installed Cargo packages.".into(),
        ));
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
fn get_installed_snap_packages() -> Result<Vec<String>, AppError> {
    let mut packages = Vec::new();
    let output = Command::new("snap").arg("list").output()?;

    if !output.status.success() {
        return Err(AppError::Other(
            "Failed to list installed Snap packages.".into(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        // Example line: "Name Version Rev Tracking Publisher Notes"
        if let Some(pkg_name) = line.split_whitespace().next() {
            packages.push(pkg_name.to_string());
        }
    }
    Ok(packages)
}

// Helper to get installed Flatpak packages
fn get_installed_flatpak_packages() -> Result<Vec<String>, AppError> {
    let output = Command::new("flatpak")
        .arg("list")
        .arg("--app") // Only list applications
        .arg("--columns=application") // Specify the column to get application IDs
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Other(
            format!("Failed to list installed Flatpak packages: {}", stderr).into(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Filter out empty lines and collect the application IDs
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

// Helper function to confirm installation with the user
fn confirm_installation(prompt: &str) -> Result<bool, AppError> {
    print!("{} (y/N): ", prompt);
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

fn apply_config(
    config: &Config,
    dry_run: bool,
    yes: bool,
    only: Option<Vec<String>>,
) -> Result<(), AppError> {
    // Helper closure to check if a section should be processed
    let should_process = |section_name: &str| -> bool {
        match &only {
            Some(sections) => sections
                .iter()
                .any(|s| s.eq_ignore_ascii_case(section_name)),
            None => true, // Process all sections if 'only' is not specified
        }
    };

    // Handle system updates
    if should_process("system") {
        if let Some(sys) = &config.system {
            if sys.update {
                if dry_run {
                    println!("Would run: sudo apt update");
                } else {
                    run_command("sudo", &["apt", "update"])?;
                }
            }
        }
    }

    // Execute APT commands
    if should_process("apt") {
        if let Some(apt) = &config.apt {
            // APT package installation is not easily parallelized due to sudo and potential dependencies.
            // We process them sequentially for now.
            for pkg_spec in &apt.list {
                let mut pkg_name = pkg_spec.as_str();
                let mut desired_version: Option<String> = None;

                // Check if a specific version is requested
                if let Some((name, version)) = pkg_spec.split_once('=') {
                    pkg_name = name;
                    desired_version = Some(version.to_string());
                }

                // Check if package is already installed
                let is_installed = Command::new("dpkg")
                    .arg("-s")
                    .arg(pkg_name)
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);

                if is_installed {
                    if let Some(version_to_match) = &desired_version {
                        match get_installed_apt_version(pkg_name) {
                            Ok(Some(installed_version)) => {
                                if installed_version == *version_to_match {
                                    println!(
                                        "APT package '{}' version '{}' already installed, skipping.",
                                        pkg_name, installed_version
                                    );
                                    continue; // Skip installation if version matches
                                } else {
                                    println!("APT package '{}' installed with version '{}', but '{}' is requested. Reinstalling.", pkg_name, installed_version, version_to_match);
                                    // Proceed to installation
                                }
                            }
                            Ok(None) => {
                                // This case should ideally not happen if is_installed is true, but handle defensively.
                                eprintln!("Warning: APT package '{}' reported as installed but version query failed. Proceeding with installation.", pkg_name);
                                // Proceed to installation
                            }
                            Err(e) => {
                                eprintln!("Warning: Error checking installed APT version for '{}': {}. Proceeding with installation.", pkg_name, e);
                                // Proceed to installation
                            }
                        }
                    } else {
                        // No version specified, and package is installed. Skip.
                        println!("APT package '{}' already installed, skipping.", pkg_name);
                        continue;
                    }
                } else {
                    // Package is not installed.
                    if desired_version.is_some() {
                        println!(
                            "APT package '{}' version '{}' not installed. Installing.",
                            pkg_name,
                            desired_version.as_ref().unwrap()
                        );
                    } else {
                        println!("APT package '{}' not installed. Installing.", pkg_name);
                    }
                    // Proceed to installation
                }

                // If we reach here, installation is needed.
                let action_desc = format!("Installing APT package '{}'", pkg_spec);
                log_or_eprint(&action_desc, "Failed to log message");
                println!("{}", action_desc);

                if dry_run {
                    println!("Would run: sudo apt install -y {}", pkg_spec);
                } else {
                    if !yes
                        && !confirm_installation(&format!("Do you want to install '{}'?", pkg_spec))?
                    {
                        println!("Installation aborted by user.");
                        continue; // Skip this package
                    }
                    run_command("sudo", &["apt", "install", "-y", pkg_spec])?;
                }
            }
        }
    }

    // Execute Snap commands
    if should_process("snap") {
        if let Some(snap) = &config.snap {
            let packages_to_install: Vec<_> = snap
                .list
                .iter()
                .filter(|pkg| {
                    let pkg_name = pkg.split_whitespace().next().unwrap_or(pkg);
                    if !is_snap_package_installed(pkg_name) {
                        true
                    } else {
                        println!("Snap package '{}' already installed, skipping.", pkg_name);
                        false
                    }
                })
                .collect();

            if packages_to_install.is_empty() {
                return Ok(());
            }

            if dry_run {
                for pkg in packages_to_install {
                    println!("Would run: sudo snap install {}", pkg);
                }
                return Ok(());
            }

            if !yes {
                // Sequential confirmation
                for pkg in &packages_to_install {
                    if confirm_installation(&format!(
                        "Do you want to install snap package '{}'?",
                        pkg
                    ))? {
                        run_command("sudo", &["snap", "install", pkg])?;
                    } else {
                        println!("Installation aborted by user.");
                    }
                }
            } else {
                // Parallel installation
                packages_to_install
                    .par_iter()
                    .try_for_each(|pkg| run_command("sudo", &["snap", "install", pkg]).map_err(AppError::Command))?;
            }
        }
    }

    // Execute Flatpak commands
    if should_process("flatpak") {
        if let Some(flatpak) = &config.flatpak {
            if dry_run {
                // For dry run, process sequentially for better readability
                for pkg in &flatpak.list {
                    if !is_flatpak_package_installed(pkg) {
                        let command_str = format!("flatpak install -y {}", pkg);
                        println!("Would run: {}", command_str);
                    } else {
                        println!("Flatpak package '{}' already installed, skipping.", pkg);
                    }
                }
            } else {
                // When user confirmation is required, the installation loop for these packages must be sequential.
                if !yes {
                    for pkg in &flatpak.list {
                        if !is_flatpak_package_installed(pkg) {
                            if confirm_installation(&format!(
                                "Do you want to install flatpak package '{}'?",
                                pkg
                            ))? {
                                run_command("flatpak", &["install", "-y", pkg])?;
                            } else {
                                println!("Installation aborted by user.");
                            }
                        } else {
                            println!("Flatpak package '{}' already installed, skipping.", pkg);
                        }
                    }
                } else {
                    // Parallel execution for actual installation
                    let result: Result<(), AppError> = flatpak.list.par_iter().try_for_each(|pkg| {
                        if !is_flatpak_package_installed(pkg) {
                            run_command("flatpak", &["install", "-y", pkg]).map_err(AppError::Command)
                        } else {
                            println!("Flatpak package '{}' already installed, skipping.", pkg);
                            Ok(())
                        }
                    });
                    result?;
                }
            }
        }
    }

    // Execute Cargo install commands in parallel, propagating errors
    if should_process("cargo") {
        if let Some(cargo) = &config.cargo {
            let packages_to_install: Vec<_> = cargo
                .list
                .iter()
                .filter(|pkg| {
                    if !is_cargo_package_installed(pkg) {
                        true
                    } else {
                        println!("Cargo package '{}' already installed, skipping.", pkg);
                        false
                    }
                })
                .collect();

            if packages_to_install.is_empty() {
                return Ok(());
            }

            if dry_run {
                for pkg in packages_to_install {
                    println!("Would run: cargo install --locked --force {}", pkg);
                }
                return Ok(());
            }

            // Cargo install doesn't typically prompt for confirmation, so no 'yes' check needed here.
            packages_to_install
                .par_iter()
                .try_for_each(|pkg| run_command("cargo", &["install", "--locked", "--force", pkg]).map_err(AppError::Command))?;
        }
    }

    // Handle .deb files
    if should_process("deb") {
        if let Some(deb) = &config.deb {
            let temp_dir = tempdir()?;
            let client = Client::new(); // Client created here, outside the loop
            for url in &deb.urls {
                let filename = url.split('/').next_back().unwrap_or("package.deb");
                let temp_path = temp_dir.path().join(filename);

                println!("Downloading {} to {}", url, temp_path.display());
                let mut response = client.get(url).send()?;
                if !response.status().is_success() {
                    return Err(AppError::Other(
                        format!("Failed to download {}: {}", url, response.status()).into(),
                    ));
                }
                let mut file = fs::File::create(&temp_path)?;
                response.copy_to(&mut file)?;

                println!("Installing {}...", temp_path.display());
                if dry_run {
                    println!("Would run: sudo dpkg -i {}", temp_path.display());
                    println!("Would run: sudo apt --fix-broken install -y");
                } else {
                    if !yes
                        && !confirm_installation(&format!(
                            "Do you want to install deb package '{}'?",
                            url
                        ))?
                    {
                        println!("Installation aborted by user.");
                        continue; // Skip this package
                    }
                    run_command(
                        "sudo",
                        &[
                            "dpkg",
                            "-i",
                            temp_path
                                .to_str()
                                .ok_or(AppError::Other("Temporary path is not valid UTF-8".into()))?,
                        ],
                    )?;
                    run_command("sudo", &["apt", "--fix-broken", "install", "-y"])?;
                }
            }
        }
    }

    Ok(())
}

fn run_scripts(config: &Config, script_name: &str, is_remote_source: bool) -> Result<(), AppError> {
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
            return Err(AppError::Other(
                format!("Script '{}' not found.", script_name).into(),
            ));
        }
    } else {
        eprintln!("No [scripts] section found in the TOML configuration.");
        return Err(AppError::Other("No [scripts] section found.".into()));
    }
    Ok(())
}

// Function to export the current environment to a TOML manifest
fn export_current_environment() -> Result<Config, AppError> {
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

// Helper function to check package discrepancies for a given package manager
fn check_package_discrepancies(
    package_manager_name: &str,
    toml_packages: &std::collections::HashSet<&str>,
    installed_packages: &std::collections::HashSet<&str>,
) {
    // Packages in TOML but not installed
    let missing: Vec<_> = toml_packages.difference(installed_packages).collect();
    if !missing.is_empty() {
        println!(
            "\n{} packages listed in TOML but not installed:",
            package_manager_name
        );
        for pkg in missing {
            println!("- {}", pkg);
        }
    }

    // Packages installed but not in TOML
    let extra: Vec<_> = installed_packages.difference(toml_packages).collect();
    if !extra.is_empty() {
        println!(
            "\n{} packages installed but not listed in TOML:",
            package_manager_name
        );
        for pkg in extra {
            println!("- {}", pkg);
        }
    }
}

// Function to perform the doctor command logic
fn doctor_command(config: &Config, source: &str) -> Result<(), AppError> {
    println!("Running railtube doctor for: {}", source);

    // Check APT packages
    if let Some(apt_section) = &config.apt {
        let toml_packages = apt_section
            .list
            .iter()
            .map(|pkg_spec| pkg_spec.split('=').next().unwrap_or(pkg_spec))
            .collect::<std::collections::HashSet<_>>();
        let installed_packages = get_installed_apt_packages()?;
        let installed_packages_set = installed_packages
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        check_package_discrepancies("APT", &toml_packages, &installed_packages_set);
    }

    // Check Snap packages
    if let Some(snap_section) = &config.snap {
        let toml_packages = snap_section
            .list
            .iter()
            .map(String::as_str) // Convert &String to &str for HashSet comparison
            .collect::<std::collections::HashSet<_>>();
        let installed_packages = get_installed_snap_packages()?;
        let installed_packages_set = installed_packages
            .iter()
            .map(String::as_str) // Convert &String to &str for HashSet comparison
            .collect::<std::collections::HashSet<_>>();
        check_package_discrepancies("Snap", &toml_packages, &installed_packages_set);
    }

    // Check Flatpak packages
    if let Some(flatpak_section) = &config.flatpak {
        let toml_packages = flatpak_section
            .list
            .iter()
            .map(String::as_str) // Convert &String to &str for HashSet comparison
            .collect::<std::collections::HashSet<_>>();
        let installed_packages = get_installed_flatpak_packages()?;
        let installed_packages_set = installed_packages
            .iter()
            .map(String::as_str) // Convert &String to &str for HashSet comparison
            .collect::<std::collections::HashSet<_>>();
        check_package_discrepancies("Flatpak", &toml_packages, &installed_packages_set);
    }

    // Check Cargo packages
    if let Some(cargo_section) = &config.cargo {
        let toml_packages = cargo_section
            .list
            .iter()
            .map(String::as_str) // Convert &String to &str for HashSet comparison
            .collect::<std::collections::HashSet<_>>();
        let installed_packages = get_installed_cargo_packages()?;
        let installed_packages_set = installed_packages
            .iter()
            .map(String::as_str) // Convert &String to &str for HashSet comparison
            .collect::<std::collections::HashSet<_>>();
        check_package_discrepancies("Cargo", &toml_packages, &installed_packages_set);
    }

    Ok(())
}

fn main() -> Result<(), AppError> {
    let args = Args::parse();

    // Handle the Export command separately as it exits early
    if let Commands::Export { ref output } = args.command {
        let exported_config = export_current_environment()?;
        let toml_string =
            toml::to_string_pretty(&exported_config).map_err(|e| AppError::Other(Box::new(e)))?;

        // Add comment for unexported sections
        let mut final_toml_string = String::new();
        final_toml_string.push_str("# NOTE: scripts and deb sections are not exported as they are defined, not installed.\n");
        final_toml_string.push_str(&toml_string);

        let mut file = fs::File::create(output)?;
        file.write_all(final_toml_string.as_bytes())?;
        println!("Environment exported to {}", output);
        return Ok(()); // Exit after export
    }

    // For other commands, fetch and parse the TOML configuration
    let config: Config = match &args.command {
        Commands::Apply { ref source, .. }
        | Commands::Run { ref source, .. }
        | Commands::Doctor { ref source } => {
            let toml_str = fetch_toml_content(source)?;
            toml::from_str(&toml_str).map_err(AppError::TomlDe)?
        }
        // Export command is handled above, so this arm should not be reached.
        // If it were, it would indicate a logic error.
        Commands::Export { .. } => unreachable!("Export command handled separately"),
    };

    // Determine if the source was a URL for logging purposes before args.command is moved
    let is_remote_source = if let Commands::Run { source, .. } = &args.command {
        source.starts_with("http://") || source.starts_with("https://")
    } else {
        false // Should not happen in this arm
    };

    // Execute the appropriate command logic
    match args.command {
        Commands::Apply {
            dry_run,
            yes,
            only: args_only,
            .. // Ignore source as it's already used to load config
        } => {
            apply_config(&config, dry_run, yes, args_only)?;
        }
        Commands::Doctor { ref source } => {
            // The config is already loaded above.
            doctor_command(&config, source)?;
        }
        Commands::Run {
            ref script_name,
            .. // Ignore source as it's already used to load config
        } => {
            run_scripts(&config, script_name, is_remote_source)?;
        }
        Commands::Export { .. } => {
            // This case is handled before the match, so it should be unreachable.
            unreachable!("Export command handled separately");
        }
    };

    Ok(())
}
