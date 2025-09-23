use clap::{Parser, Subcommand};

/// Railtube: Declarative OS Package Management
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
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
