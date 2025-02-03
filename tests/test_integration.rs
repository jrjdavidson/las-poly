use assert_cmd::Command;
use serde_json::Value;
use std::fs;

// use predicates::prelude::*;
use tempfile::TempDir;

fn setup() -> TempDir {
    TempDir::new().expect("failed to create temp dir")
}
#[test]
fn test_real_folder_detailed() {
    let tempdir = setup();
    let output_path = tempdir.path().join("output.geojson");
    let data_folder = "tests/data";

    let mut cmd = Command::cargo_bin("las-poly").unwrap();
    cmd.arg(data_folder)
        .arg(&output_path)
        .arg("--use-detailed-outline")
        .arg("--group-by-folder")
        .arg("--recurse")
        .arg("--guess-crs")
        .assert()
        .success();

    // Check if the output file exists
    assert!(output_path.exists());

    // Check the content of the GeoJSON file
    let geojson_content = fs::read_to_string(output_path).unwrap();

    // Parse the GeoJSON content
    let geojson: Value = serde_json::from_str(&geojson_content).unwrap();

    // Validate the structure
    assert_eq!(geojson["type"], "FeatureCollection");
    assert!(geojson["features"].is_array());

    // Check that the features array is not empty
    let features = geojson["features"].as_array().unwrap();
    assert!(!features.is_empty());

    // Validate the structure of the first feature
    let first_feature = &features[0];
    assert_eq!(first_feature["type"], "Feature");
    assert!(first_feature["geometry"].is_object());
    assert!(first_feature["properties"].is_object());

    // Validate the geometry type and coordinates
    let geometry = first_feature["geometry"].as_object().unwrap();
    assert_eq!(geometry["type"], "Polygon");
    assert!(geometry["coordinates"].is_array());

    // Check the length of the coordinates array
    let coordinates = geometry["coordinates"].as_array().unwrap();
    assert!(!coordinates.is_empty());
    let first_ring = coordinates[0].as_array().unwrap();
    assert!(first_ring.len() > 3); // A valid polygon should have at least 4 points

    // Check specific properties within the first feature
    let properties = first_feature["properties"].as_object().unwrap();
    assert_eq!(properties["SourceFileDir"], "tests/data");
    assert!(properties["number_of_features"].is_number());
    assert!(properties["number_of_points"].is_number());
    assert!(properties["date"].is_string());
    assert!(properties["generating_software"].is_string());
    assert!(properties["system_identifier"].is_string());
    assert!(properties["version"].is_string());

    // Validate the format of specific properties
    let date = properties["date"].as_str().unwrap();
    assert!(date.parse::<chrono::NaiveDate>().is_ok());

    let generating_software = properties["generating_software"].as_str().unwrap();
    assert!(!generating_software.is_empty());

    let system_identifier = properties["system_identifier"].as_str().unwrap();
    assert!(!system_identifier.is_empty());

    let version = properties["version"].as_str().unwrap();
    assert!(!version.is_empty());
}
#[test]
fn test_real_folder_merged() {
    let tempdir = setup();
    let output_path = tempdir.path().join("output.geojson");
    let data_folder = "tests/data";

    let mut cmd = Command::cargo_bin("las-poly").unwrap();
    cmd.arg(data_folder)
        .arg(&output_path)
        .arg("--merge-if-overlap")
        .arg("--recurse")
        .arg("--guess-crs")
        .assert()
        .success();

    // Check if the output file exists
    assert!(output_path.exists());

    // Check the content of the GeoJSON file
    let geojson_content = fs::read_to_string(output_path).unwrap();

    // Parse the GeoJSON content
    let geojson: Value = serde_json::from_str(&geojson_content).unwrap();

    // Validate the structure
    assert_eq!(geojson["type"], "FeatureCollection");
    assert!(geojson["features"].is_array());

    // Check that the features array is not empty
    let features = geojson["features"].as_array().unwrap();
    assert!(!features.is_empty());

    // Validate the structure of the first feature
    let first_feature = &features[0];
    assert_eq!(first_feature["type"], "Feature");
    assert!(first_feature["geometry"].is_object());
    assert!(first_feature["properties"].is_object());

    // Validate the geometry type and coordinates
    let geometry = first_feature["geometry"].as_object().unwrap();
    assert_eq!(geometry["type"], "Polygon");
    assert!(geometry["coordinates"].is_array());

    // Check the length of the coordinates array
    let coordinates = geometry["coordinates"].as_array().unwrap();
    assert!(!coordinates.is_empty());
    let first_ring = coordinates[0].as_array().unwrap();
    assert!(first_ring.len() > 3); // A valid polygon should have at least 4 points

    // Check specific properties within the first feature
    let properties = first_feature["properties"].as_object().unwrap();
    assert_eq!(properties["SourceFileDir"], "tests/data");
    assert!(properties["number_of_features"].is_number());
    assert!(properties["number_of_points"].is_number());
    assert!(properties["date"].is_string());
    assert!(properties["generating_software"].is_string());
    assert!(properties["system_identifier"].is_string());
    assert!(properties["version"].is_string());

    // Validate the format of specific properties
    let date = properties["date"].as_str().unwrap();
    assert!(date.parse::<chrono::NaiveDate>().is_ok());

    let generating_software = properties["generating_software"].as_str().unwrap();
    assert!(!generating_software.is_empty());

    let system_identifier = properties["system_identifier"].as_str().unwrap();
    assert!(!system_identifier.is_empty());

    let version = properties["version"].as_str().unwrap();
    assert!(!version.is_empty());
}
