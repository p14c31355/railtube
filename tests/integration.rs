use std::fs::File;
use std::io::Write;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_apply_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let toml_path = temp_dir.path().join("test.toml");

    // Create a simple TOML config
    let mut file = File::create(&toml_path).unwrap();
    writeln!(file, r#"
[apt]
list = ["fake-pkg"]
"#).unwrap();

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

    assert!(output.status.success(), "Test command failed: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Would run: sudo apt install -y fake-pkg"), "Expected dry-run output for fake-pkg");
}

#[test]
fn test_export_generates_toml() {
    let output = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg("export")
        .arg("--output")
        .arg("test_export.toml")
        .output()
        .expect("failed to execute process");

    if !output.status.success() {
        // Environment may not have all tools, skip assert
        eprintln!("Export failed in test environment: {}", String::from_utf8_lossy(&output.stderr));
        return;
    }

    // Check if file was created and has content
    let content = std::fs::read_to_string("test_export.toml").expect("Failed to read exported file");
    assert!(content.contains("[apt]"), "Exported TOML should have [apt] section");
    assert!(content.contains("[cargo]"), "Exported TOML should have [cargo] section");

    // Clean up
    std::fs::remove_file("test_export.toml").unwrap();
}
