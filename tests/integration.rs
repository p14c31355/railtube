use std::fs::File;
use std::io::Write;
use std::process::Command;
use tempfile::TempDir;
use std::ops::Drop;

struct FileGuard(&'static str);

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0);
    }
}

#[test]
fn test_apply_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let toml_path = temp_dir.path().join("test.toml");

    // Create a simple TOML config
    let mut file = File::create(&toml_path).unwrap();
    writeln!(
        file,
        r#"
[apt]
list = ["fake-pkg"]
"#
    )
    .unwrap();

    // Run cargo run -- apply --source test.toml --dry-run
    let output = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("apply")
        .arg("--source")
        .arg(toml_path.to_str().unwrap())
        .arg("--dry-run")
        .output()
        .expect("failed to execute process");

    assert!(
        output.status.success(),
        "Test command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Would run: sudo apt install -y fake-pkg"),
        "Expected dry-run output for fake-pkg"
    );
}

#[test]
fn test_export_generates_toml() {
    const TEST_FILE: &str = "test_export.toml";
    let _guard = FileGuard(TEST_FILE);

    // Check prerequisites
    let mut prerequisites_met = true;

    // Check dpkg-query for APT
    if let Err(e) = std::process::Command::new("dpkg-query").arg("-W").arg("-f=${Package}\\n").status() {
        eprintln!("APT prerequisite failed: {}", e);
        prerequisites_met = false;
    }

    // Check snap list
    if let Err(e) = std::process::Command::new("snap").arg("list").status() {
        eprintln!("Snap prerequisite failed: {}", e);
        prerequisites_met = false;
    }

    // Check flatpak list
    if let Err(e) = std::process::Command::new("flatpak").arg("list").status() {
        eprintln!("Flatpak prerequisite failed: {}", e);
        prerequisites_met = false;
    }

    // Check cargo install --list
    if let Err(e) = std::process::Command::new("cargo").arg("install").arg("--list").status() {
        eprintln!("Cargo prerequisite failed: {}", e);
        prerequisites_met = false;
    }

    if !prerequisites_met {
        eprintln!("Skipping export test due to missing prerequisites in test environment.");
        return;
    }

    let output = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("export")
        .arg("--output")
        .arg(TEST_FILE)
        .output()
        .expect("failed to execute process");

    assert!(
        output.status.success(),
        "Export command failed. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Check if file was created and has content
    let content =
        std::fs::read_to_string(TEST_FILE).expect("Failed to read exported file");
    assert!(
        content.contains("[apt]"),
        "Exported TOML should have [apt] section"
    );
    assert!(
        content.contains("[cargo]"),
        "Exported TOML should have [cargo] section"
    );
}
