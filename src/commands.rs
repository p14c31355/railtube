use crate::config::{Config, Section, SystemSection};
use crate::errors::AppError;
use crate::package::*;
use crate::utils::{confirm_installation, run_command};
use rayon::prelude::*;
use reqwest::blocking::Client;
use std::collections::HashSet;
use tempfile::tempdir;

pub fn apply_config(
    config: &Config,
    dry_run: bool,
    yes: bool,
    only: Option<Vec<String>>,
) -> Result<(), AppError> {
    let should_process = |section_name: &str| -> bool {
        match &only {
            Some(sections) => sections
                .iter()
                .any(|s| s.eq_ignore_ascii_case(section_name)),
            None => true,
        }
    };

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

    if should_process("apt") {
        if let Some(apt) = &config.apt {
            let apt_map = match crate::package::get_installed_apt_packages_map() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Warning: Error fetching APT packages map: {}. Proceeding with installation for all APT packages.", e);
                    std::collections::HashMap::new()
                }
            };
            for pkg_spec in &apt.list {
                let (pkg_name, desired_version) =
                    if let Some((name, version)) = pkg_spec.split_once('=') {
                        (name, Some(version.to_string()))
                    } else {
                        (pkg_spec.as_str(), None)
                    };

                let should_install = crate::package::determine_package_installation(
                    pkg_name,
                    &desired_version,
                    apt_map.get(pkg_name),
                    "APT",
                );

                if !should_install {
                    continue;
                }

                let action_desc = format!("Installing APT package '{}'", pkg_spec);
                crate::utils::log_or_eprint(&action_desc, "Failed to log message");
                println!("{}", action_desc);

                if dry_run {
                    println!("Would run: sudo apt install -y {}", pkg_spec);
                } else {
                    if !yes
                        && !confirm_installation(&format!(
                            "Do you want to install '{}'?",
                            pkg_spec
                        ))?
                    {
                        println!("Installation aborted by user.");
                        continue;
                    }
                    run_command("sudo", &["apt", "install", "-y", pkg_spec])?;
                }
            }
        }
    }

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

            if !packages_to_install.is_empty() {
                if dry_run {
                    for pkg in &packages_to_install {
                        println!("Would run: sudo snap install {}", pkg);
                    }
                } else if !yes {
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
                    packages_to_install.par_iter().try_for_each(|pkg| {
                        run_command("sudo", &["snap", "install", pkg]).map_err(AppError::Command)
                    })?;
                }
            }
        }
    }

    if should_process("flatpak") {
        if let Some(flatpak) = &config.flatpak {
            let packages_to_install: Vec<_> = flatpak
                .list
                .iter()
                .filter(|pkg| {
                    if !is_flatpak_package_installed(pkg) {
                        true
                    } else {
                        println!("Flatpak package '{}' already installed, skipping.", pkg);
                        false
                    }
                })
                .collect();

            if !packages_to_install.is_empty() {
                if dry_run {
                    for pkg in &packages_to_install {
                        println!("Would run: flatpak install -y {}", pkg);
                    }
                } else if !yes {
                    for pkg in &packages_to_install {
                        if confirm_installation(&format!(
                            "Do you want to install flatpak package '{}'?",
                            pkg
                        ))? {
                            run_command("flatpak", &["install", "-y", pkg])?;
                        } else {
                            println!("Installation aborted by user.");
                        }
                    }
                } else {
                    packages_to_install.par_iter().try_for_each(|pkg| {
                        run_command("flatpak", &["install", "-y", pkg]).map_err(AppError::Command)
                    })?;
                }
            }
        }
    }

    if should_process("cargo") {
        if let Some(cargo) = &config.cargo {
            let cargo_map = match crate::package::get_installed_cargo_packages_map() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Warning: Error fetching Cargo packages map: {}. Proceeding with installation for all Cargo packages.", e);
                    std::collections::HashMap::new()
                }
            };
            for pkg_spec in &cargo.list {
                let (pkg_name, desired_version) =
                    if let Some((name, version)) = pkg_spec.split_once('=') {
                        (name, Some(version.to_string()))
                    } else {
                        (pkg_spec.as_str(), None)
                    };

                let should_install = crate::package::determine_package_installation(
                    pkg_name,
                    &desired_version,
                    cargo_map.get(pkg_name),
                    "Cargo",
                );

                if should_install {
                    if dry_run {
                        println!("Would run: cargo install --locked --force {}", pkg_spec);
                    } else {
                        run_command("cargo", &["install", "--locked", "--force", pkg_spec])?;
                    }
                }
            }
        }
    }

    if should_process("deb") {
        if let Some(deb) = &config.deb {
            let temp_dir = tempdir()?;
            let client = Client::new();
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
                let mut file = std::fs::File::create(&temp_path)?;
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
                        continue;
                    }
                    run_command(
                        "sudo",
                        &[
                            "dpkg",
                            "-i",
                            temp_path.to_str().ok_or(AppError::Other(
                                "Temporary path is not valid UTF-8".into(),
                            ))?,
                        ],
                    )?;
                    run_command("sudo", &["apt", "--fix-broken", "install", "-y"])?;
                }
            }
        }
    }

    Ok(())
}

pub fn run_scripts(
    config: &Config,
    script_name: &str,
    is_remote_source: bool,
) -> Result<(), AppError> {
    if let Some(scripts) = &config.scripts {
        if let Some(command_to_run) = scripts.commands.get(script_name) {
            println!("Running script '{}': {}", script_name, command_to_run);

            if is_remote_source {
                println!("WARNING: Executing script from a remote source.");
                if !confirm_installation("Do you want to proceed?")? {
                    println!("Script execution aborted by user.");
                    return Ok(());
                }
            }

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

pub fn export_current_environment() -> Result<Config, AppError> {
    let config = Config {
        system: Some(SystemSection { update: false }),
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
        deb: None,
        scripts: None,
    };

    Ok(config)
}

pub fn check_package_discrepancies(
    package_manager_name: &str,
    toml_packages: &HashSet<&str>,
    installed_packages: &HashSet<&str>,
) {
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

fn check_section_discrepancies<F, P>(
    section: &Option<Section>,
    manager_name: &str,
    get_installed: F,
    parse_pkg: P,
) where
    F: FnOnce() -> Result<Vec<String>, AppError>,
    P: Fn(&String) -> &str,
{
    if let Some(section) = section {
        let toml_packages = section.list.iter().map(parse_pkg).collect::<HashSet<_>>();
        match get_installed() {
            Ok(installed_packages) => {
                let installed_packages_set = installed_packages
                    .iter()
                    .map(String::as_str)
                    .collect::<HashSet<_>>();
                check_package_discrepancies(manager_name, &toml_packages, &installed_packages_set);
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to list installed {} packages: {}",
                    manager_name, e
                );
            }
        }
    }
}

pub fn doctor_command(config: &Config, source: &str) -> Result<(), AppError> {
    println!("Running railtube doctor for: {}", source);

    check_section_discrepancies(&config.apt, "APT", get_installed_apt_packages, |pkg_spec| {
        pkg_spec.split('=').next().unwrap_or(pkg_spec.as_str())
    });

    check_section_discrepancies(&config.snap, "Snap", get_installed_snap_packages, |pkg| {
        pkg.split_whitespace().next().unwrap_or(pkg.as_str())
    });

    check_section_discrepancies(
        &config.flatpak,
        "Flatpak",
        get_installed_flatpak_packages,
        |pkg| pkg.as_str(),
    );

    check_section_discrepancies(
        &config.cargo,
        "Cargo",
        get_installed_cargo_packages,
        |pkg| pkg.split('=').next().unwrap_or(pkg.as_str()),
    );

    Ok(())
}
