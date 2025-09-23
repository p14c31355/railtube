use crate::errors::AppError;
use std::collections::HashMap;
use std::process::Command;

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

pub fn get_installed_apt_packages_map() -> Result<HashMap<String, String>, AppError> {
    let output = Command::new("dpkg-query")
        .arg("-W")
        .arg("-f=${Package} ${Version}\\n")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Other(
            format!(
                "Failed to list installed APT packages with versions: {}",
                stderr
            )
            .into(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut map = HashMap::new();
    for line in stdout.lines() {
        if let Some((pkg, ver)) = line.split_once(' ') {
            map.insert(pkg.to_string(), ver.to_string());
        }
    }
    Ok(map)
}

pub fn get_installed_cargo_packages_map() -> Result<HashMap<String, String>, AppError> {
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
    let mut map = HashMap::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let pkg_name = parts[0].trim_end_matches(':');
            let version = parts[1].trim_start_matches('v');
            map.insert(pkg_name.to_string(), version.to_string());
        }
    }
    Ok(map)
}

pub fn determine_package_installation(
    pkg_name: &str,
    desired_version: &Option<String>,
    installed_version: Option<&String>,
    package_type: &str,
) -> bool {
    if let Some(installed_version) = installed_version {
        if let Some(version_to_match) = desired_version {
            if installed_version != version_to_match {
                println!(
                    "{} package '{}' installed with version '{}', but '{}' is requested. Reinstalling.",
                    package_type, pkg_name, installed_version, version_to_match
                );
                true
            } else {
                println!(
                    "{} package '{}' version '{}' already installed, skipping.",
                    package_type, pkg_name, installed_version
                );
                false
            }
        } else {
            println!(
                "{} package '{}' already installed, skipping.",
                package_type, pkg_name
            );
            false
        }
    } else {
        if let Some(version) = desired_version {
            println!(
                "{} package '{}' version '{}' not installed. Installing.",
                package_type, pkg_name, version
            );
        } else {
            println!(
                "{} package '{}' not installed. Installing.",
                package_type, pkg_name
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_install_not_installed_no_version() {
        let result = determine_package_installation("testpkg", &None, None, "Test");
        assert!(result);
    }

    #[test]
    fn test_determine_install_not_installed_with_version() {
        let result =
            determine_package_installation("testpkg", &Some("1.0".to_string()), None, "Test");
        assert!(result);
    }

    #[test]
    fn test_determine_skip_installed_no_desired() {
        let installed = "1.0".to_string();
        let result = determine_package_installation("testpkg", &None, Some(&installed), "Test");
        assert!(!result);
    }

    #[test]
    fn test_determine_skip_installed_matching_version() {
        let installed = "1.0".to_string();
        let result = determine_package_installation(
            "testpkg",
            &Some("1.0".to_string()),
            Some(&installed),
            "Test",
        );
        assert!(!result);
    }

    #[test]
    fn test_determine_install_installed_mismatching_version() {
        let installed = "1.0".to_string();
        let result = determine_package_installation(
            "testpkg",
            &Some("2.0".to_string()),
            Some(&installed),
            "Test",
        );
        assert!(result);
    }
}
