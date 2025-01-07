//! A library for processing LAS files and generating GeoJSON polygons.
//!
//! This library provides functionality to process folders containing LAS files,
//! generate polygons from the LAS data, and save the results as a GeoJSON file.
//!
//! # Examples
//!
//! ```rust
//! use las_poly::process_folder;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     process_folder("path/to/folder", true, true, true, None)?;
//!     Ok(())
//! }
//! ```

mod crs_utils;
use crs_utils::{extract_crs, extract_crs_from_geotiff, Crs, CrsError};
use geo::{ConvexHull, Coord, LineString, Polygon};
use las::Reader;
use serde::Serialize;
use serde_json::Map;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use thiserror::Error;
use threadpool::ThreadPool;
use walkdir::WalkDir;

use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};

/// Processes a folder containing LAS files and generates GeoJSON polygons.
///
/// # Arguments
///
/// * `folder_path` - The path to the folder containing LAS files.
/// * `use_detailed_outline` - Whether to use detailed outlines for the polygons.
/// * `group_by_folder` - Whether to group the polygons by folder.
/// * `recurse` - Whether to recurse into subdirectories.
/// * `output_file` - Optional output file name. If not provided, a default name will be used.
///
/// # Returns
///
/// This function returns a `Result` indicating success or failure.
///
/// # Examples
///
/// ```rust
/// use las_poly::process_folder;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     process_folder("path/to/folder", true, true, true, Some("output.geojson"))?;
///     Ok(())
/// }
/// ```
#[derive(Error, Debug)]
pub enum PolygonError {
    #[error("Failed to read LAS file: {0}")]
    LasError(#[from] las::Error),
    #[error("Failed to transform coordinates: {0}")]
    ProjError(#[from] proj::ProjError),
    #[error("Failed to extract CRS: {0}")]
    CrsError(#[from] CrsError),
    #[error("Failed to create output file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to create Proj instance: {0}")]
    ProjCreateError(#[from] proj::ProjCreateError),
}
pub fn process_folder(
    folder_path: &str,
    use_detailed_outline: bool,
    group_by_folder: bool,
    recurse: bool,             // New parameter to control recursion
    output_file: Option<&str>, // New parameter for output file name
) -> Result<(), PolygonError> {
    let num_threads = num_cpus::get();
    println!("Number of threads used: {:?}", num_threads);

    let pool = ThreadPool::new(num_threads);
    let (tx, rx) = mpsc::channel();

    // Spawn a thread to walk through the directory and send file paths
    let folder_path_string = folder_path.to_string();
    thread::spawn(move || {
        let walker = if recurse {
            WalkDir::new(folder_path_string).into_iter()
        } else {
            WalkDir::new(folder_path_string).max_depth(1).into_iter()
        };

        for entry in walker.filter_map(Result::ok) {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("las") {
                let file_path = entry.path().to_str().unwrap().to_string();
                tx.send(file_path).unwrap();
            }
        }
    });

    let (feature_tx, feature_rx) = mpsc::channel();

    // Spawn threads to process each LAS file
    for file_path in rx {
        let feature_tx = feature_tx.clone();
        pool.execute(move || {
            // println!("Creating read thread for {:?}", file_path);

            match create_polygon(&file_path, use_detailed_outline) {
                Ok(feature) => {
                    feature_tx.send(feature).unwrap();
                    println!("Successfully created polygon for :{:?} ", file_path);
                }
                Err(e) => {
                    println!("Error in thread {:?}: {:?}", file_path, e);
                }
            }
        });
    }

    drop(feature_tx); // Close the channel to signal completion

    let mut features = Vec::new();

    // Collect features from the channel
    if group_by_folder {
        let mut features_by_folder: HashMap<String, Vec<Geometry>> = HashMap::new();
        for feature in feature_rx {
            let folder_path = feature
                .properties
                .as_ref()
                .unwrap()
                .get("SourceFileDir")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let geometry = feature.geometry.unwrap();

            features_by_folder
                .entry(folder_path)
                .or_default()
                .push(geometry);
        }

        // Merge geometries for each folder path
        for (folder_path, geometries) in features_by_folder {
            let merged_polygon = geometries.into_iter().fold(
                Polygon::new(LineString::new(vec![]), vec![]),
                |acc, geometry| {
                    if let Value::Polygon(geom_coords) = geometry.value {
                        let mut coords: Vec<Coord<f64>> = acc.exterior().clone().into_inner();
                        let new_coords: Vec<Coord<f64>> = geom_coords[0]
                            .iter()
                            .map(|c| Coord { x: c[0], y: c[1] })
                            .collect();
                        coords.extend(new_coords);

                        // Create a LineString from the combined coordinates
                        let line_string = LineString::from(coords);

                        // Compute the convex hull to get a single enclosing polygon
                        line_string.convex_hull()
                    } else {
                        acc
                    }
                },
            );

            let exterior_coords: Vec<Vec<f64>> = merged_polygon
                .exterior()
                .coords()
                .map(|c| vec![c.x, c.y])
                .collect();
            let geojson_polygon = Value::Polygon(vec![exterior_coords]);
            let geometry = Geometry::new(geojson_polygon);
            let mut properties = Map::new();
            properties.insert("SourceFileDir".to_string(), folder_path.into());

            let feature = Feature {
                geometry: Some(geometry),
                properties: Some(properties),
                id: None,
                bbox: None,
                foreign_members: None,
            };

            features.push(feature);
        }
    } else {
        for feature in feature_rx {
            features.push(feature);
        }
    }

    // Create a FeatureCollection from all the merged features
    let feature_collection = FeatureCollection {
        features,
        bbox: None,
        foreign_members: None,
    };

    let geojson = GeoJson::FeatureCollection(feature_collection);

    // Determine the output file name
    let path = Path::new(folder_path);
    let file_stem = path
        .file_name()
        .unwrap_or_else(|| path.components().last().unwrap().as_os_str());
    let binding = format!("{}.geojson", file_stem.to_string_lossy());
    let output_file_name = output_file.unwrap_or(&binding);

    // Save the GeoJSON to a file
    println!("{:?}", output_file_name);
    let mut file = File::create(output_file_name)?;
    file.write_all(geojson.to_string().as_bytes())?;

    println!("Merged polygons saved to {}", output_file_name);

    Ok(())
}

use proj::Proj;

/// Creates a polygon from a LAS file.
///
/// # Arguments
///
/// * `file_path` - The path to the LAS file.
/// * `use_detailed_outline` - Whether to use detailed outlines for the polygons.
///
/// # Returns
///
/// This function returns a `Result` containing a `Feature` or an error.
///
/// # Examples
///
/// ```rust
/// use las_poly::create_polygon;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let feature = create_polygon("tests/data/input1.las", true)?;
///     Ok(())
/// }
/// ```
///
///

#[derive(Serialize)]
struct FeatureProperties<'a> {
    filename: String,
    folder_path: Option<&'a Path>,
    number_of_points: u64,
    date: Option<String>,
    file_source_id: u16,
    generating_software: String,
    version: String,
    system_identifier: String,
}

impl FeatureProperties<'_> {
    fn to_map(&self) -> Map<String, serde_json::Value> {
        let mut map: Map<String, serde_json::Value> = Map::new();
        map.insert("SourceFile".to_string(), self.filename.clone().into());
        if let Some(folder_path) = self.folder_path {
            map.insert(
                "SourceFileDir".to_string(),
                folder_path.to_string_lossy().into(),
            );
        }
        map.insert("number_of_points".to_string(), self.number_of_points.into());
        if let Some(ref date) = self.date {
            map.insert("date".to_string(), date.clone().into());
        }
        map.insert("file_source_id".to_string(), self.file_source_id.into());
        map.insert(
            "generating_software".to_string(),
            self.generating_software.clone().into(),
        );
        map.insert("version".to_string(), self.version.clone().into());
        map.insert(
            "system_identifier".to_string(),
            self.system_identifier.clone().into(),
        );
        map
    }
}

pub fn create_polygon(
    file_path: &str,
    use_detailed_outline: bool,
) -> Result<Feature, PolygonError> {
    // Open the LAS file
    let crs = match extract_crs(file_path)? {
        // Check the CRS of the LAS file
        Some(Crs::Wkt(wkt)) => Some(wkt),
        Some(Crs::GeoTiff(geo_key_directory, geo_double_params, geo_ascii_params)) => {
            let crs = extract_crs_from_geotiff(
                &geo_key_directory,
                geo_double_params.as_deref(),
                geo_ascii_params.as_deref(),
            )?;
            Some(crs)
        }
        None => {
            println!("No CRS found. Will assume EPSG:4326 (i.e., will not transform data)");
            None
        }
    };

    // Create a Proj instance for transforming coordinates to EPSG:4326
    let to_epsg4326 = Proj::new_known_crs(
        crs.unwrap_or_else(|| "EPSG:4326".to_string())
            .trim_end_matches(char::from(0)),
        "EPSG:4326",
        None,
    )
    .map_err(PolygonError::from)?;

    let mut reader = Reader::from_path(file_path)?;

    let geojson_polygon = if !use_detailed_outline {
        // Use the header to create a faster outline of data
        let bounds = reader.header().bounds();
        let exterior_coords = vec![
            to_epsg4326
                .convert((bounds.min.x, bounds.min.y))
                .unwrap_or((bounds.min.x, bounds.min.y)),
            to_epsg4326
                .convert((bounds.max.x, bounds.min.y))
                .unwrap_or((bounds.max.x, bounds.min.y)),
            to_epsg4326
                .convert((bounds.max.x, bounds.max.y))
                .unwrap_or((bounds.max.x, bounds.max.y)),
            to_epsg4326
                .convert((bounds.min.x, bounds.max.y))
                .unwrap_or((bounds.min.x, bounds.max.y)),
            to_epsg4326
                .convert((bounds.min.x, bounds.min.y))
                .unwrap_or((bounds.min.x, bounds.min.y)),
        ]
        .into_iter()
        .map(|(x, y)| vec![x, y])
        .collect();
        Value::Polygon(vec![exterior_coords])
    } else {
        // Collect points
        let points: Vec<Coord<f64>> = reader
            .points()
            .filter_map(Result::ok)
            .map(|p| {
                let (x, y) = to_epsg4326.convert((p.x, p.y)).unwrap_or((p.x, p.y));
                Coord { x, y }
            })
            .collect();

        // Create a LineString from the points
        let line_string = LineString::from(points);

        // Compute the convex hull
        let convex_hull: Polygon<f64> = line_string.convex_hull();

        // Convert the convex hull to GeoJSON
        let exterior_coords: Vec<Vec<f64>> = convex_hull
            .exterior()
            .coords()
            .map(|c| vec![c.x, c.y])
            .collect();
        Value::Polygon(vec![exterior_coords])
    };
    let geometry = Geometry::new(geojson_polygon);

    // Extract folder path from file path
    let folder_path = Path::new(file_path).parent();

    // Add additional properties from the LAS header
    let header = reader.header();
    let properties = FeatureProperties {
        filename: file_path.to_string(),
        folder_path,
        number_of_points: header.number_of_points(),
        date: header.date().map(|d| d.to_string()),
        file_source_id: header.file_source_id(),
        generating_software: header.generating_software().to_string(),
        version: format!("{}.{}", header.version().major, header.version().minor),
        system_identifier: header.system_identifier().to_string(),
    };

    // Convert the properties struct to a map
    let properties_map = properties.to_map();

    let feature = Feature {
        geometry: Some(geometry),
        properties: Some(properties_map),
        id: None,
        bbox: None,
        foreign_members: None,
    };

    Ok(feature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn setup() -> tempfile::TempDir {
        tempdir().expect("Failed to create temporary directory")
    }

    #[test]
    fn test_create_polygon_simple_outline() {
        let file_path = "tests/data/input1.las";
        let result = create_polygon(file_path, false);
        assert!(result.is_ok());
        let feature = result.unwrap();
        assert!(feature.geometry.is_some());

        // Additional assertions
        let geometry = feature.geometry.unwrap();
        if let geojson::Value::Polygon(polygon) = geometry.value {
            assert_eq!(polygon.len(), 1); // Ensure there's one polygon
            assert_eq!(polygon[0].len(), 5); // Ensure the polygon has 5 points (including the closing point)
            assert_eq!(polygon[0][0], [174.91941143911868, -36.87566977961954]);
            assert_eq!(polygon[0][1], [174.92268177317487, -36.87561689771632]);
            assert_eq!(polygon[0][2], [174.92264691906135, -36.874226826185556]);
            assert_eq!(polygon[0][3], [174.91937664420047, -36.87427970543262]);
            assert_eq!(polygon[0][4], [174.91941143911868, -36.87566977961954]);
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
        let result = create_polygon(file_path, true);
        assert!(result.is_ok());
        let feature = result.unwrap();
        assert!(feature.geometry.is_some());

        // Additional assertions
        let geometry = feature.geometry.unwrap();
        if let geojson::Value::Polygon(polygon) = &geometry.value {
            assert_eq!(polygon.len(), 1); // Ensure there's one polygon
            assert_eq!(polygon[0].len(), 42); // Ensure the polygon has 30 points (including the closing point)
            assert_eq!(polygon[0][0], [174.92264798671903, -36.874263591726894]); // Check the first point
            assert_eq!(polygon[0][1], [174.9226633846416, -36.87488308028734]); // Check the second point
            assert_eq!(polygon[0][2], [174.92266550472735, -36.874967634735846]); // Check the third point
            assert_eq!(polygon[0][3], [174.92267523622655, -36.875355747295856]); // Check the fourth point
            assert_eq!(polygon[0][29], [174.92646576488872, -36.87489574250899]);
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
    fn test_process_folder_no_group_by_folder() {
        let tempdir = setup();
        let output_path = tempdir.path().join("data.geojson");
        let folder_path = "tests/data";

        let result = process_folder(
            folder_path,
            true,
            false,
            true,
            Some(output_path.to_str().unwrap()),
        );
        assert!(result.is_ok());

        // Check if the output file is created
        assert!(output_path.exists());

        // Read the file and perform checks
        let geojson_str = fs::read_to_string(&output_path).unwrap();
        let geojson: GeoJson = geojson_str.parse().unwrap();
        if let GeoJson::FeatureCollection(fc) = geojson {
            assert_eq!(fc.features.len(), 2); // Ensure there are two features

            // Check the first feature
            let feature1 = &fc.features[0];
            assert!(feature1.geometry.is_some());
            let geometry1 = feature1.geometry.as_ref().unwrap();
            if let geojson::Value::Polygon(polygon) = &geometry1.value {
                assert_eq!(polygon.len(), 1); // Ensure there's one polygon
                assert_eq!(polygon[0].len(), 24); // Ensure the polygon has 24 points (including the closing point)
            } else {
                panic!("Expected a Polygon geometry for feature1");
            }
            let expected_path: &Path = Path::new("tests/data/input1.las");

            assert_eq!(
                feature1
                    .properties
                    .as_ref()
                    .unwrap()
                    .get("SourceFile")
                    .unwrap()
                    .as_str()
                    .map(Path::new),
                Some(expected_path)
            );

            // Check the second feature
            let feature2 = &fc.features[1];
            assert!(feature2.geometry.is_some());
            let geometry2 = feature2.geometry.as_ref().unwrap();
            if let geojson::Value::Polygon(polygon) = &geometry2.value {
                assert_eq!(polygon.len(), 1);
                assert_eq!(polygon[0].len(), 42); // Adjust the number of points as needed
            } else {
                panic!("Expected a Polygon geometry for feature2");
            }
            let expected_path: &Path = Path::new("tests/data/input2.las");
            assert_eq!(
                feature2
                    .properties
                    .as_ref()
                    .unwrap()
                    .get("SourceFile")
                    .unwrap()
                    .as_str()
                    .map(Path::new),
                Some(expected_path)
            );
        } else {
            panic!("Expected a FeatureCollection");
        }
    }

    #[test]
    fn test_integration_workflow_group_by_folder() {
        let temp_dir = setup();
        let output_path = temp_dir.path().join("data.geojson");
        let folder_path = "tests/data";
        let result = process_folder(
            folder_path,
            true,
            true,
            true,
            Some(output_path.to_str().unwrap()),
        );
        println!("{:?}", result);

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

                // Check specific coordinates (e.g., the first and last)
                assert_eq!(
                    exterior_ring[0],
                    vec![174.91942109783082, -36.87566929909413]
                );
                assert_eq!(
                    exterior_ring[24],
                    vec![174.9264345357605, -36.87488206215996]
                );
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
        let result = process_folder(
            temp_dir.path().to_str().unwrap(),
            true,
            true,
            false,
            Some(output_path.to_str().unwrap()),
        );
        assert!(result.is_ok());

        // Check if the output file is created
        assert!(output_path.exists());

        // Read the file and perform checks
        let geojson_str = fs::read_to_string(output_path).unwrap();
        let geojson: GeoJson = geojson_str.parse().unwrap();
        println!("{:?}", geojson);
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
                assert_eq!(exterior_ring.len(), 3); // Adjust the number of points as needed

                // Check specific coordinates (e.g., the first and last)
                assert_eq!(exterior_ring[0], vec![1.0, 2.0]); // Mock coordinates
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

        let result = create_polygon(current_dir_file_path.to_str().unwrap(), false);
        println!("{:?}", result);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_las_file() {
        let temp_dir = setup();
        let current_dir_file_path = temp_dir.path().join("invalid.las");
        let mut file = File::create(&current_dir_file_path).unwrap();
        file.write_all(b"Invalid LAS data").unwrap();

        let result = create_polygon(current_dir_file_path.to_str().unwrap(), false);
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
                    x: -100.0,
                    y: -300.0,
                    z: -400.0,
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
        let result = create_polygon(file_path.to_str().unwrap(), true);
        println!("{:?}", result);
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
    fn test_crs_transformation() {
        let file_path = "tests/crs/BQ29_1000_4907.las";

        let result = create_polygon(file_path, false);
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
}
