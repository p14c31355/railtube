# Railtube: Declarative OS Package Management with Cargo

Railtube is a Rust-based command-line tool that allows you to manage your operating system's packages and run scripts declaratively using TOML manifests. It aims to simplify environment setup and reproducibility by integrating with common package managers like APT, Snap, Flatpak, Cargo, and handling `.deb` files, as well as executing custom scripts.

## Motivation

Inspired by the desire for a unified and reproducible environment setup, Railtube leverages the familiarity of Cargo and the readability of TOML. It provides a single command to define and apply your desired system configuration, making it easier to set up new machines, share environments, or manage dotfiles.

## Features

*   **Declarative Configuration**: Define your desired packages, their sources, and scripts in a TOML file.
*   **Multi-Package Manager Support**: Manages packages for:
    *   **APT**: Installs packages using `sudo apt install -y`.
    *   **Snap**: Installs packages using `sudo snap install`.
    *   **Flatpak**: Installs packages using `flatpak install -y`.
    *   **Cargo**: Installs Rust crates using `cargo install`.
    *   **`.deb` files**: Downloads `.deb` packages from URLs and installs them, handling dependency issues.
*   **Script Execution**: Run custom shell scripts defined in the TOML manifest.
*   **URL Support**: Fetch TOML configurations directly from URLs (e.g., GitHub Gists).
*   **System Updates**: Option to run `apt update` before APT package installations.
*   **Standalone Executable**: Installs as a standalone `railtube` command.

## Installation

1.  **Prerequisites**:
    *   Rust toolchain (including `cargo`) installed.
    *   `sudo` access for package installations.
    *   `apt`, `snap`, `flatpak`, `dpkg`, and a shell (`sh`) installed on your system.

2.  **Build and Install `railtube`**:
    Navigate to the project's root directory in your terminal and run:
    ```bash
    cargo build
    ```
    Then, install the `railtube` executable globally:
    ```bash
    cargo install --path .
    ```
    This makes the `railtube` command available in your terminal.

## Usage

Railtube provides four subcommands: `apply`, `run`, `doctor`, and `export`.

### `railtube apply`

Applies package installations defined in a TOML manifest. Skips already installed packages (with optional version checking).

```bash
railtube apply --source <path_or_url> [--dry-run] [--yes] [--only <sections>]
```

- `--dry-run`: Show what would be installed without executing commands.
- `--yes`: Skip confirmation prompts.
- `--only <sections>`: Apply only specific sections (comma-separated, e.g., `apt,cargo`).

### `railtube run`

Executes a specific script defined in the `[scripts]` section of a TOML manifest.

```bash
railtube run --source <path_or_url> <script_name>
```

### `railtube doctor`

Checks for discrepancies between the packages listed in the TOML manifest and those currently installed on the system.

```bash
railtube doctor --source <path_or_url>
```

This command reports:
- Packages in TOML but not installed (missing).
- Installed packages not listed in TOML (extra).

### `railtube export`

Exports the current installed packages (from APT, Snap, Flatpak, Cargo) to a TOML manifest file. Note: Scripts and deb sections are not exported as they are declarative, not queryable from the system.

```bash
railtube export [--output <file>]
```

- `--output`: Path for the output TOML file (default: `exported-env.toml`).

### TOML Manifest Format

The TOML file defines different sections for various package managers and scripts.

```toml
# Example railtube.toml

# Optional: Run 'apt update' before installing apt packages
[system]
update = true

# APT packages (supports version pinning: "package=1.2.3")
[apt]
list = [
    "git",
    "vim",
    "curl",
    "htop",
]

# Snap packages
[snap]
list = [
    "spotify",
    "code --classic", # Example of snap install with arguments
]

# Flatpak packages
[flatpak]
list = [
    "com.discordapp.Discord",
    "org.mozilla.firefox",
]

# Cargo packages to install globally
[cargo]
list = [
    "ripgrep",
    "bat",
]

# URLs for .deb packages to download and install
[deb]
urls = [
    "https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb",
    "https://download.slack-edge.com/linux_releases/slack-desktop-4.29.149-amd64.deb",
]

# Scripts to run
[scripts]
setup-dev-env = "echo 'Setting up development environment...' && git config --global --add --bool push.default simple"
update-all = "echo 'Updating all systems...' && sudo apt update && sudo apt upgrade -y && cargo install-update -a"
```

### Examples

*   **Apply configuration from a local file**:
    ```bash
    railtube apply --source ./my-dev-env.toml
    ```

*   **Run a script from a local TOML file**:
    ```bash
    railtube run --source ./my-dev-env.toml setup-dev-env
    ```

*   **Apply configuration from a URL (e.g., a GitHub Gist)**:
    ```bash
    railtube apply --source https://gist.github.com/yourusername/yourgistid/raw/yourfile.toml
    ```

*   **Run a script from a URL**:
    ```bash
    railtube run --source https://gist.github.com/yourusername/yourgistid/raw/yourfile.toml update-all
    ```

## Future Enhancements

*   **Dotfiles Management**: Support for cloning and applying dotfiles from Git repositories.
*   **Dependency Resolution**: More advanced dependency management beyond `apt --fix-broken install`.
*   **Rollback Functionality**: Ability to uninstall packages or revert environment changes.
*   **Remote Registries**: Support for custom package registries.
*   **Cross-Platform Support**: Extend to manage packages on macOS (Homebrew) and Windows (Winget).

## Contributing

See [CONTRIBUTING.md](docs/CONTRIBUTING.md) for details. Contributions are welcome! Please open an issue or pull request on GitHub.

## License

This project is licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](docs/LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](docs/LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
