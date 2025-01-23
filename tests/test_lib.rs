use approx::assert_abs_diff_eq;
use geojson::{GeoJson, Value};
use las::{Header, Point, Writer};
use las_poly::{create_polygon, process_folder, ProcessConfig};
use proj::Proj;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use test_log::test;

fn setup() -> tempfile::TempDir {
    tempdir().expect("Failed to create temporary directory")
}

fn normalize_path(path: &str) -> String {
    PathBuf::from(path).to_string_lossy().replace("\\", "/")
}

fn create_las_file(file_path: &str, points: Vec<Point>) {
    let header = Header::default();
    let mut writer = Writer::from_path(file_path, header).unwrap();
    for point in points {
        writer.write_point(point).unwrap();
    }
}

fn create_laz_file(file_path: &str, points: Vec<Point>) {
    let header = Header::default();
    let mut writer = Writer::from_path(file_path, header).unwrap();
    for point in points {
        writer.write_point(point).unwrap();
    }
}

#[test]
fn test_create_polygon_simple_outline() {
    let file_path = "tests/data/input1.las";
    let result = create_polygon(file_path, false, true);
    assert!(result.is_ok());
    let feature = result.unwrap();
    assert!(feature.geometry.is_some());

    // Additional assertions
    let geometry = feature.geometry.unwrap();
    if let geojson::Value::Polygon(polygon) = geometry.value {
        assert_eq!(polygon.len(), 1); // Ensure there's one polygon
        assert_eq!(polygon[0].len(), 5); // Ensure the polygon has 5 points (including the closing point)
        assert_abs_diff_eq!(polygon[0][0][0], 174.91941143911868, epsilon = 1e-10);
        assert_abs_diff_eq!(polygon[0][0][1], -36.87566977961954, epsilon = 1e-10);
        assert_abs_diff_eq!(polygon[0][1][0], 174.92268177317487, epsilon = 1e-10);
        assert_abs_diff_eq!(polygon[0][1][1], -36.87561689771632, epsilon = 1e-10);
        assert_abs_diff_eq!(polygon[0][2][0], 174.92264691906135, epsilon = 1e-10);
        assert_abs_diff_eq!(polygon[0][2][1], -36.874226826185556, epsilon = 1e-10);
        assert_abs_diff_eq!(polygon[0][3][0], 174.91937664420047, epsilon = 1e-10);
        assert_abs_diff_eq!(polygon[0][3][1], -36.87427970543262, epsilon = 1e-10);
        assert_abs_diff_eq!(polygon[0][4][0], 174.91941143911868, epsilon = 1e-10);
        assert_abs_diff_eq!(polygon[0][4][1], -36.87566977961954, epsilon = 1e-10);
    } else {
        panic!("Expected a Polygon geometry");
    }

    // Check properties
    let properties = feature.properties.unwrap();
    assert_eq!(
        properties.get("SourceFile").unwrap(),
        "tests/data/input1.las"
    );
    assert_eq!(properties.get("SourceFileDir").unwrap(), "tests/data");
    assert_eq!(properties.get("number_of_points").unwrap(), 97359);
}

#[test]
fn test_create_polygon_convex_hull() {
    let file_path = "tests/data/input2.las";
    let result = create_polygon(file_path, true, true);
    assert!(result.is_ok());
    let feature = result.unwrap();
    assert!(feature.geometry.is_some());

    // Additional assertions
    let geometry = feature.geometry.unwrap();
    if let geojson::Value::Polygon(polygon) = &geometry.value {
        assert_eq!(polygon.len(), 1); // Ensure there's one polygon
        assert_eq!(polygon[0].len(), 42); // Ensure the polygon has 30 points (including the closing point)

        assert_abs_diff_eq!(polygon[0][0][0], 174.92264798671903, epsilon = 1e-10); // Check the first point's x-coordinate
        assert_abs_diff_eq!(polygon[0][0][1], -36.874263591726894, epsilon = 1e-10); // Check the first point's y-coordinate

        assert_abs_diff_eq!(polygon[0][1][0], 174.9226633846416, epsilon = 1e-10); // Check the second point's x-coordinate
        assert_abs_diff_eq!(polygon[0][1][1], -36.87488308028734, epsilon = 1e-10); // Check the second point's y-coordinate

        assert_abs_diff_eq!(polygon[0][2][0], 174.92266550472735, epsilon = 1e-10); // Check the third point's x-coordinate
        assert_abs_diff_eq!(polygon[0][2][1], -36.874967634735846, epsilon = 1e-10); // Check the third point's y-coordinate

        assert_abs_diff_eq!(polygon[0][3][0], 174.92267523622655, epsilon = 1e-10); // Check the fourth point's x-coordinate
        assert_abs_diff_eq!(polygon[0][3][1], -36.875355747295856, epsilon = 1e-10); // Check the fourth point's y-coordinate

        assert_abs_diff_eq!(polygon[0][29][0], 174.92646576488872, epsilon = 1e-10); // Check the 30th point's x-coordinate
        assert_abs_diff_eq!(polygon[0][29][1], -36.87489574250899, epsilon = 1e-10);
    // Check the 30th point's y-coordinate
    // Check the closing point
    } else {
        panic!("Expected a Polygon geometry");
    }

    // Check properties
    let properties = feature.properties.unwrap();
    assert_eq!(
        properties.get("SourceFile").unwrap(),
        "tests/data/input2.las"
    );
    assert_eq!(properties.get("SourceFileDir").unwrap(), "tests/data");
}

#[test]
fn test_geotiff_crs() {
    let file_path = "tests/crs/merged.las";
    let result = create_polygon(file_path, true, true);
    assert!(result.is_ok(), "Failed to create polygon from LAS file");
    let feature = result.unwrap();
    assert!(feature.geometry.is_some(), "Feature geometry is missing");

    // Additional assertions
    let geometry = feature.geometry.unwrap();
    if let geojson::Value::Polygon(polygon) = &geometry.value {
        assert_eq!(polygon.len(), 1, "Expected one polygon"); // Ensure there's one polygon
    } else {
        panic!("Expected a Polygon geometry");
    }

    // Check properties
    let properties = feature.properties.unwrap();
    assert_eq!(
        properties.get("SourceFile").unwrap(),
        "tests/crs/merged.las"
    );
    assert_eq!(properties.get("SourceFileDir").unwrap(), "tests/crs");
}

#[test]
fn test_process_folder_no_group_by_folder() {
    let tempdir = setup();
    let output_path = tempdir.path().join("data.geojson");
    let folder_path = "tests/data";

    let config = ProcessConfig {
        folder_path: folder_path.to_string(),
        use_detailed_outline: true,
        group_by_folder: false,
        merge_tiled: false,
        merge_if_overlap: false,
        recurse: true,
        guess_crs: true,
        output_file: Some(output_path.to_str().unwrap().to_string()),
    };

    let result = process_folder(config);
    assert!(result.is_ok());

    // Check if the output file is created
    assert!(output_path.exists());

    // Read the file and perform checks
    let geojson_str = fs::read_to_string(&output_path).unwrap();
    let geojson: GeoJson = geojson_str.parse().unwrap();
    if let GeoJson::FeatureCollection(fc) = geojson {
        assert_eq!(fc.features.len(), 2); // Ensure there are two features

        // Check the features
        let feature1 = fc
            .features
            .iter()
            .find(|f| {
                let source_file = f
                    .properties
                    .as_ref()
                    .unwrap()
                    .get("SourceFile")
                    .unwrap()
                    .as_str()
                    .unwrap();
                normalize_path(source_file) == normalize_path("tests/data/input1.las")
            })
            .expect("Expected feature for input1.las");

        let feature2 = fc
            .features
            .iter()
            .find(|f| {
                let source_file = f
                    .properties
                    .as_ref()
                    .unwrap()
                    .get("SourceFile")
                    .unwrap()
                    .as_str()
                    .unwrap();
                normalize_path(source_file) == normalize_path("tests/data/input2.las")
            })
            .expect("Expected feature for input2.las");

        // Check the first feature
        assert!(feature1.geometry.is_some());
        let geometry1 = feature1.geometry.as_ref().unwrap();
        if let geojson::Value::Polygon(polygon) = &geometry1.value {
            assert_eq!(polygon.len(), 1); // Ensure there's one polygon
            assert_eq!(polygon[0].len(), 24); // Ensure the polygon has 24 points (including the closing point)
        } else {
            panic!("Expected a Polygon geometry for feature1");
        }

        // Check the second feature
        assert!(feature2.geometry.is_some());
        let geometry2 = feature2.geometry.as_ref().unwrap();
        if let geojson::Value::Polygon(polygon) = &geometry2.value {
            assert_eq!(polygon.len(), 1);
            assert_eq!(polygon[0].len(), 42); // Adjust the number of points as needed
        } else {
            panic!("Expected a Polygon geometry for feature2");
        }
    } else {
        panic!("Expected a FeatureCollection");
    }
}

#[test]
fn test_integration_workflow_group_by_folder() {
    let temp_dir = setup();
    let output_path = temp_dir.path().join("data.geojson");
    let folder_path = "tests/data";

    let config = ProcessConfig {
        folder_path: folder_path.to_string(),
        use_detailed_outline: true,
        group_by_folder: true,
        merge_tiled: false,
        merge_if_overlap: false,
        recurse: true,
        guess_crs: true,
        output_file: Some(output_path.to_str().unwrap().to_string()),
    };

    let result = process_folder(config);

    assert!(result.is_ok());

    // Check if the output file is created
    assert!(temp_dir.path().exists());

    // Read the file and perform checks
    let geojson_str = fs::read_to_string(output_path).unwrap();
    let geojson: GeoJson = geojson_str.parse().unwrap();
    if let GeoJson::FeatureCollection(fc) = geojson {
        assert!(!fc.features.is_empty());

        // Check the number of features
        assert_eq!(fc.features.len(), 1);

        // Check the properties of the first feature
        let feature = &fc.features[0];
        assert!(feature.properties.is_some());
        let properties = feature.properties.as_ref().unwrap();

        let expected_path = Path::new("tests/data");
        let folder_path = properties.get("SourceFileDir").unwrap().as_str().unwrap();
        assert_eq!(Path::new(folder_path), expected_path);

        // Check the geometry of the first feature
        assert!(feature.geometry.is_some());
        let geometry = feature.geometry.as_ref().unwrap();
        if let Value::Polygon(coords) = &geometry.value {
            // Check the number of coordinate sets (should be 1 for a simple polygon)
            assert_eq!(coords.len(), 1);

            // Check the number of coordinates in the exterior ring
            let exterior_ring = &coords[0];
            assert_eq!(exterior_ring.len(), 37);
            // Check specific coordinates (e.g., the first and last) with approximate comparison
            assert_abs_diff_eq!(exterior_ring[0][0], 174.91942109783082, epsilon = 1e-10);
            assert_abs_diff_eq!(exterior_ring[0][1], -36.87566929909413, epsilon = 1e-10);
            assert_abs_diff_eq!(exterior_ring[24][0], 174.9264345357605, epsilon = 1e-10);
            assert_abs_diff_eq!(exterior_ring[24][1], -36.87488206215996, epsilon = 1e-10);
            // Check specific coordinates (e.g., the first and last)
        } else {
            panic!("Expected Polygon geometry");
        }
    } else {
        panic!("Expected FeatureCollection");
    }

    // Clean up
}

#[test]
fn test_process_folder_group_by_folder_missing_sourcefiledir() {
    let temp_dir = setup();
    let output_path = temp_dir.path().join("data.geojson");
    use las::{Point, Writer};

    // Create a mock LAS file in the current working directory
    let current_dir_file_path = temp_dir.path().join("mock_root_file.las");
    {
        let mut writer = Writer::from_path(&current_dir_file_path, Default::default()).unwrap();

        let point1 = Point {
            x: 1.,
            y: 2.,
            z: 3.,
            ..Default::default()
        };
        let point2 = Point {
            x: 2.,
            y: 3.,
            z: 4.,
            ..Default::default()
        };
        let point3 = Point {
            x: 4.,
            y: 5.,
            z: 6.,
            ..Default::default()
        };
        writer.write_point(point1).unwrap();
        writer.write_point(point2).unwrap();
        writer.write_point(point3).unwrap();
    }

    let config = ProcessConfig {
        folder_path: temp_dir.path().to_str().unwrap().to_string(),
        use_detailed_outline: false,
        group_by_folder: true,
        merge_tiled: false,
        merge_if_overlap: false,
        recurse: true,
        guess_crs: true,
        output_file: Some(output_path.to_str().unwrap().to_string()),
    };

    let result = process_folder(config);
    assert!(result.is_ok());

    // Check if the output file is created
    assert!(output_path.exists());

    // Read the file and perform checks
    let geojson_str = fs::read_to_string(output_path).unwrap();
    let geojson: GeoJson = geojson_str.parse().unwrap();
    if let GeoJson::FeatureCollection(fc) = geojson {
        assert!(!fc.features.is_empty());

        // Check the number of features
        assert_eq!(fc.features.len(), 1);

        // Check the properties of the first feature
        let feature = &fc.features[0];
        assert!(feature.properties.is_some());
        let properties = feature.properties.as_ref().unwrap();

        // Simulate missing SourceFileDir
        assert!(properties.get("SourceFileDir").is_some());

        // Check the geometry of the first feature
        assert!(feature.geometry.is_some());
        let geometry = feature.geometry.as_ref().unwrap();
        if let Value::Polygon(coords) = &geometry.value {
            // Check the number of coordinate sets (should be 1 for a simple polygon)
            assert_eq!(coords.len(), 1);

            // Check the number of coordinates in the exterior ring
            let exterior_ring = &coords[0];
            assert_eq!(exterior_ring.len(), 5);

            // Check specific coordinates (e.g., the first and last)
            assert_eq!(exterior_ring[0], vec![4.0, 2.0]); // Mock coordinates
            assert_eq!(exterior_ring[1], vec![4.0, 5.0]); // Mock coordinates
        } else {
            panic!("Expected Polygon geometry");
        }
    } else {
        panic!("Expected FeatureCollection");
    }

    // Clean up
}

#[test]
fn test_empty_las_file() {
    let temp_dir = setup();

    // Create a mock LAS file in the current working directory
    let current_dir_file_path = temp_dir.path().join("empty.las");
    let header = las::Header::default();
    {
        las::Writer::from_path(&current_dir_file_path, header).unwrap();
    }

    let result = create_polygon(current_dir_file_path.to_str().unwrap(), false, true);
    assert!(result.is_err());
}

#[test]
fn test_invalid_las_file() {
    let temp_dir = setup();
    let current_dir_file_path = temp_dir.path().join("invalid.las");
    let mut file = File::create(&current_dir_file_path).unwrap();
    file.write_all(b"Invalid LAS data").unwrap();

    let result = create_polygon(current_dir_file_path.to_str().unwrap(), false, true);
    assert!(result.is_err());
}

#[test]
fn test_detailed_outline() {
    let temp_dir = setup();
    let file_path = temp_dir.path().join("detailed_outline.las");

    let header = las::Header::default();
    {
        let mut writer = las::Writer::from_path(&file_path, header).unwrap();
        let points = vec![
            las::Point {
                x: 10.0,
                y: 20.0,
                z: 30.0,
                ..Default::default()
            },
            las::Point {
                x: -10.0,
                y: -20.0,
                z: -30.0,
                ..Default::default()
            },
            las::Point {
                x: -10.0,
                y: 30.0,
                z: -40.0,
                ..Default::default()
            },
            las::Point {
                x: 25.0,
                y: 10.0,
                z: 0.0,
                ..Default::default()
            },
        ];
        for point in points {
            writer.write_point(point).unwrap();
        }
    }
    let result = create_polygon(file_path.to_str().unwrap(), true, true);
    assert!(result.is_ok());
    let feature = result.unwrap();
    assert!(feature.geometry.is_some());

    // Additional assertions
    let geometry = feature.geometry.unwrap();
    if let geojson::Value::Polygon(polygon) = geometry.value {
        assert_eq!(polygon.len(), 1); // Ensure there's one polygon
        assert!(polygon[0].len() > 4); // Ensure the polygon has more than 5 points for detailed outline
    } else {
        panic!("Expected a Polygon geometry");
    }
}

#[test]
fn test_crs_error_transformation() {
    let file_path = "tests/crs/210728_035051_Scanner_1.las";

    let result = create_polygon(file_path, false, true);
    assert!(result.is_err());
}

#[test]
fn test_crs_transformation() {
    let file_path = "tests/crs/BQ29_1000_4907.las";

    let result = create_polygon(file_path, false, true);
    assert!(result.is_ok());
    let feature = result.unwrap();
    assert!(feature.geometry.is_some());

    // Additional assertions
    let geometry = feature.geometry.unwrap();
    if let geojson::Value::Polygon(polygon) = geometry.value {
        assert_eq!(polygon.len(), 1); // Ensure there's one polygon
        assert!(polygon[0].len() > 4); // Ensure the polygon has more than 5 points for detailed outline
    } else {
        panic!("Expected a Polygon geometry");
    }
}

#[test]
fn test_proj_availability() {
    let proj = Proj::new_known_crs("EPSG:4326", "EPSG:3857", None);
    assert!(
        proj.is_ok(),
        "Failed to initialize the Proj instance- proj might not be porperly installed on system."
    );
}

#[test]
fn test_proj_transformation() {
    let proj = Proj::new_known_crs("EPSG:4326", "EPSG:3857", None).unwrap();
    let result = proj.convert((0.0, 0.0));
    assert!(result.is_ok(), "Failed to initialize the Proj instance");
    let (x, y) = result.unwrap();
    assert_eq!(x, 0.0);
    assert_eq!(y, 0.0);
}
#[test]
#[ignore = "for testing purposes"]
fn test_proj_with_valid_wkt() {
    let wkt = "GEOCCS[\"WGS84 Geocentric\",DATUM[\"WGS84\",SPHEROID[\"WGS84\",6378137,298.257223563,AUTHORITY[\"EPSG\",\"7030\"]],AUTHORITY[\"EPSG\",\"6326\"]],PRIMEM[\"Greenwich\",0,AUTHORITY[\"EPSG\",\"8901\"]],UNIT[\"Meter\",1,AUTHORITY[\"EPSG\",\"9001\"]],AXIS[\"X\",OTHER],AXIS[\"Y\",EAST],AXIS[\"Z\",NORTH],AUTHORITY[\"EPSG\",\"4978\"]]\0";
    let trimmed_wkt = wkt.trim_end_matches(char::from(0));

    let proj = Proj::new(trimmed_wkt);
    assert!(
        proj.is_ok(),
        "Failed to create Proj instance with valid WKT"
    );
}

#[test]
#[ignore = "network drive required"]
fn test_process_folder_with_merge_if_shared_vertex() {
    let temp_dir = setup();
    let output_path = temp_dir.path().join("data.geojson");
    let folder_path = r"\\file\Research\LidarPowerline\_VADIS\KiwiRail_August_2023\LAZ\";
    let config = ProcessConfig {
        folder_path: folder_path.to_string(),
        use_detailed_outline: false,
        group_by_folder: false,
        merge_tiled: true,
        merge_if_overlap: true,
        recurse: true,
        guess_crs: true,
        output_file: Some(output_path.to_str().unwrap().to_string()),
    };

    let result = process_folder(config);
    assert!(result.is_ok());

    // Check if the output file is created
    assert!(output_path.exists());

    // Read the GeoJSON file and assert the number of polygons
    let saved_content = fs::read_to_string(&output_path).unwrap();
    let geojson: GeoJson = saved_content.parse().unwrap();
    if let GeoJson::FeatureCollection(fc) = geojson {
        assert_eq!(fc.features.len(), 1); // Assuming the features should be merged into one
    } else {
        panic!("Expected a FeatureCollection");
    }
}

#[test]
fn test_process_folder_with_various_scenarios() {
    let temp_dir = setup();
    let folder_path = temp_dir.path().to_str().unwrap();

    // Create LAS files with points
    let file1_path = format!("{}/file1.las", folder_path);
    let file2_path = format!("{}/file2.las", folder_path);
    let file3_path = format!("{}/file3.las", folder_path);
    let file4_path = format!("{}/file4.las", folder_path);
    let file5_path = format!("{}/file5.las", folder_path);

    create_las_file(
        &file1_path,
        vec![
            Point {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                ..Default::default()
            },
            Point {
                x: 1.0,
                y: 1.0,
                z: 0.0,
                ..Default::default()
            },
        ],
    );

    create_las_file(
        &file2_path,
        vec![
            Point {
                x: 2.0,
                y: 2.0,
                z: 0.0,
                ..Default::default()
            },
            Point {
                x: 3.0,
                y: 3.0,
                z: 0.0,
                ..Default::default()
            },
        ],
    );

    create_las_file(
        &file3_path,
        vec![
            Point {
                x: 1.5,
                y: 1.5,
                z: 0.0,
                ..Default::default()
            },
            Point {
                x: 2.5,
                y: 2.5,
                z: 0.0,
                ..Default::default()
            },
        ],
    );

    create_las_file(
        &file4_path,
        vec![
            Point {
                x: 2.5,
                y: 1.5,
                z: 0.0,
                ..Default::default()
            },
            Point {
                x: 3.5001,
                y: 2.5,
                z: 0.0,
                ..Default::default()
            },
        ],
    );
    create_las_file(
        &file5_path,
        vec![
            Point {
                x: 3.501,
                y: 1.5,
                z: 0.0,
                ..Default::default()
            },
            Point {
                x: 4.5,
                y: 2.5,
                z: 0.0,
                ..Default::default()
            },
        ],
    );

    // Test merging with shared vertex
    let config = ProcessConfig {
        folder_path: folder_path.to_string(),
        use_detailed_outline: false,
        group_by_folder: false,
        merge_tiled: true,
        merge_if_overlap: false,
        recurse: true,
        guess_crs: true,
        output_file: Some(
            temp_dir
                .path()
                .join("output_shared_vertex.geojson")
                .to_str()
                .unwrap()
                .to_string(),
        ),
    };
    process_folder(config).unwrap();
    let output_path = temp_dir.path().join("output_shared_vertex.geojson");
    assert!(output_path.exists());
    let geojson_str = fs::read_to_string(&output_path).unwrap();
    let geojson: GeoJson = geojson_str.parse().unwrap();
    if let GeoJson::FeatureCollection(fc) = geojson {
        assert_eq!(fc.features.len(), 3);
    } else {
        panic!("Expected a FeatureCollection");
    }

    // Test merging with overlap
    let config = ProcessConfig {
        folder_path: folder_path.to_string(),
        use_detailed_outline: false,
        group_by_folder: false,
        merge_tiled: false,
        merge_if_overlap: true,
        recurse: true,
        guess_crs: true,
        output_file: Some(
            temp_dir
                .path()
                .join("output_overlap.geojson")
                .to_str()
                .unwrap()
                .to_string(),
        ),
    };
    process_folder(config).unwrap();
    let output_path = temp_dir.path().join("output_overlap.geojson");
    assert!(output_path.exists());
    let geojson_str = fs::read_to_string(&output_path).unwrap();
    let geojson: GeoJson = geojson_str.parse().unwrap();
    if let GeoJson::FeatureCollection(fc) = geojson {
        assert_eq!(fc.features.len(), 2);
    } else {
        panic!("Expected a FeatureCollection");
    }

    // Test merging folder
    let config = ProcessConfig {
        folder_path: folder_path.to_string(),
        use_detailed_outline: false,
        group_by_folder: true,
        merge_tiled: false,
        merge_if_overlap: false,
        recurse: true,
        guess_crs: true,
        output_file: Some(
            temp_dir
                .path()
                .join("output_shared_vertex_overlap.geojson")
                .to_str()
                .unwrap()
                .to_string(),
        ),
    };
    process_folder(config).unwrap();
    let output_path = temp_dir.path().join("output_shared_vertex_overlap.geojson");
    assert!(output_path.exists());
    let geojson_str = fs::read_to_string(&output_path).unwrap();
    let geojson: GeoJson = geojson_str.parse().unwrap();
    if let GeoJson::FeatureCollection(fc) = geojson {
        assert_eq!(fc.features.len(), 1);
    } else {
        panic!("Expected a FeatureCollection");
    }

    // Test without merging
    let config = ProcessConfig {
        folder_path: folder_path.to_string(),
        use_detailed_outline: false,
        group_by_folder: false,
        merge_tiled: false,
        merge_if_overlap: false,
        recurse: true,
        guess_crs: true,
        output_file: Some(
            temp_dir
                .path()
                .join("output_no_merge.geojson")
                .to_str()
                .unwrap()
                .to_string(),
        ),
    };
    process_folder(config).unwrap();
    let output_path = temp_dir.path().join("output_no_merge.geojson");
    assert!(output_path.exists());
    let geojson_str = fs::read_to_string(&output_path).unwrap();
    let geojson: GeoJson = geojson_str.parse().unwrap();
    if let GeoJson::FeatureCollection(fc) = geojson {
        assert_eq!(fc.features.len(), 5);
    } else {
        panic!("Expected a FeatureCollection");
    }
}

#[test]
fn test_process_folder_with_single_point_las() {
    let temp_dir = setup();
    let folder_path = temp_dir.path().to_str().unwrap();

    // Create LAS files with points
    let file1_path = format!("{}/file1.las", folder_path);

    create_las_file(
        &file1_path,
        vec![Point {
            x: 40.0,
            y: 30.0,
            z: 20.0,
            ..Default::default()
        }],
    );
    let config = ProcessConfig {
        folder_path: folder_path.to_string(),
        use_detailed_outline: false,
        group_by_folder: false,
        merge_tiled: false,
        merge_if_overlap: true,
        recurse: true,
        guess_crs: true,
        output_file: Some(
            temp_dir
                .path()
                .join("output_single_point.geojson")
                .to_str()
                .unwrap()
                .to_string(),
        ),
    };
    process_folder(config).unwrap();
    let output_path = temp_dir.path().join("output_single_point.geojson");
    assert!(output_path.exists());
    let geojson_str = fs::read_to_string(&output_path).unwrap();
    let geojson: GeoJson = geojson_str.parse().unwrap();
    if let GeoJson::FeatureCollection(fc) = geojson {
        assert_eq!(fc.features.len(), 1);
        let feature = &fc.features[0];
        if let Some(geometry) = &feature.geometry {
            if let Value::Polygon(polygon) = &geometry.value {
                assert_eq!(polygon.len(), 1); // One polygon
                assert_eq!(polygon[0].len(), 0); // Four coordinates (closing the polygon)
            } else {
                panic!("Expected an empty Polygon geometry");
            }
        } else {
            panic!("Expected a geometry in the feature");
        }
    } else {
        panic!("Expected a FeatureCollection");
    }
}

#[test]
fn test_create_polygon_from_laz() {
    let temp_dir = setup();
    let file_path = temp_dir.path().join("test.laz");
    create_laz_file(
        file_path.to_str().unwrap(),
        vec![
            Point {
                x: 10.0,
                y: 20.0,
                z: 30.0,
                ..Default::default()
            },
            Point {
                x: 0.0,
                y: 10.0,
                z: 30.0,
                ..Default::default()
            },
            Point {
                x: 10.0,
                y: 40.0,
                z: 30.0,
                ..Default::default()
            },
        ],
    );

    let result = create_polygon(file_path.to_str().unwrap(), true, true);
    assert!(result.is_ok());
    let feature = result.unwrap();
    assert!(feature.geometry.is_some());

    // Additional assertions
    let geometry = feature.geometry.unwrap();
    if let geojson::Value::Polygon(polygon) = geometry.value {
        assert_eq!(polygon.len(), 1); // Ensure there's one polygon
        assert!(polygon[0].len() >= 4); // Ensure the polygon has more than 5 points for detailed outline
    } else {
        panic!("Expected a Polygon geometry");
    }
}

#[test]
fn test_process_folder_with_laz_files() {
    let temp_dir = setup();
    let folder_path = temp_dir.path().to_str().unwrap();

    // Create LAZ files with points
    let file1_path = format!("{}/file1.laz", folder_path);
    let file2_path = format!("{}/file2.laz", folder_path);

    create_laz_file(
        &file1_path,
        vec![
            Point {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                ..Default::default()
            },
            Point {
                x: 1.0,
                y: 1.0,
                z: 0.0,
                ..Default::default()
            },
        ],
    );

    create_laz_file(
        &file2_path,
        vec![
            Point {
                x: 2.0,
                y: 2.0,
                z: 0.0,
                ..Default::default()
            },
            Point {
                x: 3.0,
                y: 3.0,
                z: 0.0,
                ..Default::default()
            },
        ],
    );

    let config = ProcessConfig {
        folder_path: folder_path.to_string(),
        use_detailed_outline: false,
        group_by_folder: false,
        merge_tiled: false,
        merge_if_overlap: false,
        recurse: true,
        guess_crs: true,
        output_file: Some(
            temp_dir
                .path()
                .join("output_laz.geojson")
                .to_str()
                .unwrap()
                .to_string(),
        ),
    };
    process_folder(config).unwrap();
    let output_path = temp_dir.path().join("output_laz.geojson");
    assert!(output_path.exists());
    let geojson_str = fs::read_to_string(&output_path).unwrap();
    let geojson: GeoJson = geojson_str.parse().unwrap();
    if let GeoJson::FeatureCollection(fc) = geojson {
        assert_eq!(fc.features.len(), 2);
    } else {
        panic!("Expected a FeatureCollection");
    }
}
