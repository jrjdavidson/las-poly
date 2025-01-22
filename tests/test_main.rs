use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::tempdir;
use test_log::test;

fn setup() -> tempfile::TempDir {
    tempdir().expect("Failed to create temporary directory")
}

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
    let tempdir = setup();
    let output_path = tempdir.path().join("output.geojson");

    let las_file_path = tempdir.path().join("dummy.las");
    fs::write(&las_file_path, b"dummy content").unwrap();

    let mut cmd = Command::cargo_bin("las-poly").unwrap();
    cmd.arg(tempdir.path())
        .arg(&output_path)
        .arg("--use-detailed-outline")
        .arg("--group-by-folder")
        .arg("--recurse")
        .arg("--guess-crs")
        .assert()
        .success();

    // Check if the output file is created
    assert!(Path::new(&output_path).exists());
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
