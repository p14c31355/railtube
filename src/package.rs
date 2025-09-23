use crate::errors::AppError;
use std::process::Command;

pub fn is_cargo_package_installed(pkg_name: &str) -> bool {
    let output = Command::new("cargo").arg("install").arg("--list").output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("Warning: Failed to list installed cargo packages. Assuming '{}' is not installed.", pkg_name);
                return false;
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.lines().any(|line| {
                line.split_whitespace()
                    .next()
                    .is_some_and(|p| p.trim_end_matches(':') == pkg_name)
            })
        }
        Err(e) => {
            eprintln!("Warning: Error executing 'cargo install --list': {}. Assuming '{}' is not installed.", e, pkg_name);
            false
        }
    }
}

pub fn get_installed_cargo_version(pkg_name: &str) -> Result<Option<String>, AppError> {
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
        if let Some(first) = line.split_whitespace().next() {
            if first.trim_end_matches(':') == pkg_name {
                // Line format: "pkg v1.2.3 (path)"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let version = parts[1].trim_start_matches('v');
                    return Ok(Some(version.to_string()));
                }
            }
        }
    }
    Ok(None)
}

pub fn is_snap_package_installed(pkg_name: &str) -> bool {
    let base_pkg_name = pkg_name.split_whitespace().next().unwrap_or(pkg_name);

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

pub fn is_flatpak_package_installed(pkg_name: &str) -> bool {
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

pub fn get_installed_apt_packages() -> Result<Vec<String>, AppError> {
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

pub fn get_installed_apt_version(pkg_name: &str) -> Result<Option<String>, AppError> {
    let output = Command::new("dpkg-query")
        .arg("-W")
        .arg("-f")
        .arg("${Version}")
        .arg(pkg_name)
        .output()?;

    if !output.status.success() {
        if String::from_utf8_lossy(&output.stderr).contains("no packages found") {
            return Ok(None);
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
    let version = stdout.trim().trim_matches('\'').to_string();
    Ok(Some(version))
}

pub fn get_installed_cargo_packages() -> Result<Vec<String>, AppError> {
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
        if let Some(pkg_name) = line.split_whitespace().next() {
            packages.push(pkg_name.to_string());
        }
    }
    Ok(packages)
}

pub fn get_installed_snap_packages() -> Result<Vec<String>, AppError> {
    let mut packages = Vec::new();
    let output = Command::new("snap").arg("list").output()?;

    if !output.status.success() {
        return Err(AppError::Other(
            "Failed to list installed Snap packages.".into(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        if let Some(pkg_name) = line.split_whitespace().next() {
            packages.push(pkg_name.to_string());
        }
    }
    Ok(packages)
}

pub fn get_installed_flatpak_packages() -> Result<Vec<String>, AppError> {
    let output = Command::new("flatpak")
        .arg("list")
        .arg("--app")
        .arg("--columns=application")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Other(
            format!("Failed to list installed Flatpak packages: {}", stderr).into(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}
