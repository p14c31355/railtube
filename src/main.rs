use clap::{Parser, Subcommand};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::{fs, io::Read, process::Command};
use tempfile::tempdir;

#[derive(Debug, Deserialize)]
struct Config {
    system: Option<SystemSection>,
    apt: Option<Section>,
    snap: Option<Section>,
    flatpak: Option<Section>,
    cargo: Option<Section>,
    deb: Option<DebSection>,
    scripts: Option<ScriptsSection>, // New section for scripts
                                     // dotfiles: Option<DotfilesSection>, // Future implementation
}

#[derive(Debug, Deserialize)]
struct SystemSection {
    update: bool,
}

#[derive(Debug, Deserialize)]
struct Section {
    list: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DebSection {
    urls: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ScriptsSection {
    #[serde(flatten)] // Use flatten to capture all key-value pairs as commands
    commands: std::collections::HashMap<String, String>,
}

// #[derive(Debug, Deserialize)]
// struct DotfilesSection {
//     repo: String,
// }

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

fn run_command(cmd: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    println!("> {} {}", cmd, args.join(" "));
    let mut command = Command::new(cmd);
    command.args(args);

    let output = command.output()?;

    if !output.status.success() {
        eprintln!("Command failed: {} {}", cmd, args.join(" "));
        eprintln!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
        eprintln!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
        return Err(format!("Command failed: {} {}", cmd, args.join(" ")).into());
    }
    Ok(())
}

fn fetch_toml_content(source: &str) -> Result<String, Box<dyn std::error::Error>> {
    if source.starts_with("http://") || source.starts_with("https://") {
        let client = Client::new();
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

fn apply_config(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    // Handle system updates
    if let Some(sys) = config.system {
        if sys.update {
            run_command("sudo", &["apt", "update"])?;
        }
    }

    // Execute APT commands
    if let Some(apt) = config.apt {
        for pkg in apt.list {
            run_command("sudo", &["apt", "install", "-y", &pkg])?;
        }
    }

    // Execute Snap commands
    if let Some(snap) = config.snap {
        for pkg in snap.list {
            run_command("sudo", &["snap", "install", &pkg])?;
        }
    }

    // Execute Flatpak commands
    if let Some(flatpak) = config.flatpak {
        for pkg in flatpak.list {
            run_command("flatpak", &["install", "-y", &pkg])?;
        }
    }

    // Execute Cargo install commands
    if let Some(cargo) = config.cargo {
        for pkg in cargo.list {
            run_command("cargo", &["install", &pkg])?;
        }
    }

    // Handle .deb files
    if let Some(deb) = config.deb {
        let temp_dir = tempdir()?;
        for url in deb.urls {
            let filename = url.split('/').next_back().unwrap_or("package.deb");
            let temp_path = temp_dir.path().join(filename);

            println!("Downloading {} to {}", url, temp_path.display());
            let client = Client::new();
            let mut response = client.get(&url).send()?;
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

fn run_scripts(config: Config, script_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(scripts) = config.scripts {
        if let Some(command_to_run) = scripts.commands.get(script_name) {
            println!("Running script '{}': {}", script_name, command_to_run);
            // Execute the command string. This is a simplified approach;
            // a more robust solution might involve parsing the command string.
            // For now, we assume it's a single command or can be executed directly.
            // This might require using a shell to interpret complex commands.
            // For simplicity, let's try to split it into command and args if possible,
            // or just execute it via a shell.
            // A safer approach for arbitrary commands is to use a shell.
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

    match args.command {
        Commands::Apply { source } => {
            let toml_str = fetch_toml_content(&source)?;
            let config: Config = toml::from_str(&toml_str)?;
            apply_config(config)?;
        }
        Commands::Run {
            source,
            script_name,
        } => {
            let toml_str = fetch_toml_content(&source)?;
            let config: Config = toml::from_str(&toml_str)?;
            run_scripts(config, &script_name)?;
        }
    }

    Ok(())
}
