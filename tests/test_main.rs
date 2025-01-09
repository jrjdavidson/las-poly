use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

#[test]
fn test_help() {
    let mut cmd = Command::cargo_bin("las-poly").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Creates a geojson file with the outlines of LAS files",
        ));
}

#[test]
fn test_process_folder() {
    let test_folder = "tests/data";
    let output_file = "tests/data/output.geojson";

    // Ensure the test folder exists
    fs::create_dir_all(test_folder).unwrap();

    // Create a dummy LAS file in the test folder
    let las_file_path = Path::new(test_folder).join("dummy.las");
    fs::write(&las_file_path, b"dummy content").unwrap();

    let mut cmd = Command::cargo_bin("las-poly").unwrap();
    cmd.arg(test_folder)
        .arg(output_file)
        .arg("--use-detailed-outline")
        .arg("--group-by-folder")
        .arg("--recurse")
        .arg("--guess-crs")
        .assert()
        .success();

    // Check if the output file is created
    assert!(Path::new(output_file).exists());

    // Clean up
    fs::remove_file(las_file_path).unwrap();
    fs::remove_file(output_file).unwrap();
}

#[test]
fn test_invalid_folder() {
    let invalid_folder = "invalid_folder";

    let mut cmd = Command::cargo_bin("las-poly").unwrap();
    cmd.arg(invalid_folder)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error"));
}
