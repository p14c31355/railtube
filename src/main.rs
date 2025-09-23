use clap::Parser;
// Added Serialize
use std::{fs, io::Write};

mod errors;
use errors::AppError;

mod config;
use config::Config;

mod cli;
use cli::{Args, Commands};

mod package;
mod utils;
use utils::*;
mod commands;
use commands::*;

// Helper function to confirm installation with the user

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
