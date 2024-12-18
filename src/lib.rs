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
//!     process_folder("path/to/folder", true, true, true)?;
//!     Ok(())
//! }
//! ```

use geo::{ConvexHull, Coord, LineString, Polygon};
use las::Reader;
use serde_json::Map;
use std::collections::HashMap;
use std::error::Error;
use std::sync::mpsc;
use std::thread;
use threadpool::ThreadPool;
use walkdir::WalkDir;

use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};
use std::fs::File;
use std::io::Write;

/// Processes a folder containing LAS files and generates GeoJSON polygons.
///
/// # Arguments
///
/// * `folder_path` - The path to the folder containing LAS files.
/// * `use_detailed_outline` - Whether to use detailed outlines for the polygons.
/// * `group_by_folder` - Whether to group the polygons by folder.
/// * `recurse` - Whether to recurse into subdirectories.
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
///     process_folder("path/to/folder", true, true, true)?;
///     Ok(())
/// }
/// ```
pub fn process_folder(
    folder_path: &str,
    use_detailed_outline: bool,
    group_by_folder: bool,
    recurse: bool, // New parameter to control recursion
) -> Result<(), Box<dyn Error>> {
    let num_threads = num_cpus::get();
    println!("Number of threads used: {:?}", num_threads);

    let pool = ThreadPool::new(num_threads);
    let (tx, rx) = mpsc::channel();

    // Spawn a thread to walk through the directory and send file paths
    let folder_path = folder_path.to_string();
    thread::spawn(move || {
        let walker = if recurse {
            WalkDir::new(folder_path).into_iter()
        } else {
            WalkDir::new(folder_path).max_depth(1).into_iter()
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
            println!("Creating read thread for {:?}", file_path);

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
                .get("folder_path")
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
            properties.insert("folder_path".to_string(), folder_path.into());

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

    // Save the GeoJSON to a file
    let mut file = File::create("las_outlines.geojson")?;
    file.write_all(geojson.to_string().as_bytes())?;

    println!("Merged polygons saved to las_outlines.geojson");

    Ok(())
}

use proj::Proj;
use std::path::Path;

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
pub fn create_polygon(
    file_path: &str,
    use_detailed_outline: bool,
) -> Result<Feature, Box<dyn Error>> {
    // Open the LAS file
    let mut reader = Reader::from_path(file_path)?;

    // Check the CRS of the LAS file
    let crs = reader
        .header()
        .vlrs()
        .iter()
        .find(|vlr| vlr.user_id == "LASF_Projection" && vlr.record_id == 34735)
        .map(|vlr| String::from_utf8_lossy(&vlr.data).to_string());

    // Create a Proj instance for transforming coordinates to EPSG:4326
    let to_epsg4326 = Proj::new_known_crs(
        &crs.unwrap_or_else(|| "EPSG:4326".to_string()),
        "EPSG:4326",
        None,
    )?;

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
    let mut properties = Map::new();
    properties.insert("filename".to_string(), file_path.to_string().into());

    // Extract folder path from file path
    if let Some(folder_path) = Path::new(file_path).parent() {
        properties.insert(
            "folder_path".to_string(),
            folder_path.to_string_lossy().into(),
        );
    }

    // Add additional properties from the LAS header
    let header = reader.header();
    properties.insert(
        "number_of_points".to_string(),
        header.number_of_points().into(),
    );
    if let Some(date) = header.date() {
        properties.insert("date".to_string(), date.to_string().into());
    }
    properties.insert("file_source_id".to_string(), header.file_source_id().into());
    properties.insert(
        "generating_software".to_string(),
        header.generating_software().into(),
    );
    properties.insert(
        "version".to_string(),
        format!("{}.{}", header.version().major, header.version().minor).into(),
    );
    properties.insert(
        "system_identifier".to_string(),
        header.system_identifier().into(),
    );

    let feature = Feature {
        geometry: Some(geometry),
        properties: Some(properties),
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

    #[test]
    fn test_create_polygon_simple_outline() {
        let file_path = "tests/data/input1.las";
        let result = create_polygon(file_path, false);
        assert!(result.is_ok());
        let feature = result.unwrap();
        println!("{:?}", feature);
        assert!(feature.geometry.is_some());

        // Additional assertions
        let geometry = feature.geometry.unwrap();
        if let geojson::Value::Polygon(polygon) = geometry.value {
            assert_eq!(polygon.len(), 1); // Ensure there's one polygon
            assert_eq!(polygon[0].len(), 5); // Ensure the polygon has 5 points (including the closing point)
            assert_eq!(polygon[0][0], [1771068.3800000001, 5917200.0]); // Check the first point
            assert_eq!(polygon[0][1], [1771359.999, 5917200.0]); // Check the second point
            assert_eq!(polygon[0][2], [1771359.999, 5917354.289]); // Check the third point
            assert_eq!(polygon[0][3], [1771068.3800000001, 5917354.289]); // Check the fourth point
            assert_eq!(polygon[0][4], [1771068.3800000001, 5917200.0]); // Check the closing point
        } else {
            panic!("Expected a Polygon geometry");
        }

        // Check properties
        let properties = feature.properties.unwrap();
        assert_eq!(properties.get("filename").unwrap(), "tests/data/input1.las");
        assert_eq!(properties.get("folder_path").unwrap(), "tests/data");
        assert_eq!(properties.get("number_of_points").unwrap(), 97359);
    }

    #[test]
    fn test_create_polygon_convex_hull() {
        let file_path = "tests/data/input2.las";
        let result = create_polygon(file_path, true);
        assert!(result.is_ok());
        let feature = result.unwrap();
        println!("{:?}", feature);
        assert!(feature.geometry.is_some());

        // Additional assertions
        let geometry = feature.geometry.unwrap();
        if let geojson::Value::Polygon(polygon) = geometry.value {
            assert_eq!(polygon.len(), 1); // Ensure there's one polygon
            assert_eq!(polygon[0].len(), 30); // Ensure the polygon has 30 points (including the closing point)
            assert_eq!(polygon[0][0], [1771360.006, 5917201.84]); // Check the first point
            assert_eq!(polygon[0][1], [1771360.026, 5917200.476]); // Check the second point
            assert_eq!(polygon[0][2], [1771360.064, 5917200.029]); // Check the third point
            assert_eq!(polygon[0][3], [1771360.307, 5917200.009]); // Check the fourth point
            assert_eq!(polygon[0][29], [1771360.006, 5917201.84]); // Check the closing point
        } else {
            panic!("Expected a Polygon geometry");
        }

        // Check properties
        let properties = feature.properties.unwrap();
        assert_eq!(properties.get("filename").unwrap(), "tests/data/input2.las");
        assert_eq!(properties.get("folder_path").unwrap(), "tests/data");
    }

    #[test]
    fn test_process_folder_no_group_by_folder() {
        let folder_path = "tests/data";
        let result = process_folder(folder_path, true, false, true);
        println!("{:?}", result);
        assert!(result.is_ok());

        // Check if the output file is created
        let output_path = Path::new("las_outlines.geojson");
        assert!(output_path.exists());

        // Read the file and perform checks
        let geojson_str = fs::read_to_string(output_path).unwrap();
        let geojson: GeoJson = geojson_str.parse().unwrap();
        if let GeoJson::FeatureCollection(fc) = geojson {
            println!("{:?}", fc);
            assert_eq!(fc.features.len(), 2); // Ensure there are two features

            // Check the first feature
            let feature1 = &fc.features[0];
            assert!(feature1.geometry.is_some());
            let geometry1 = feature1.geometry.as_ref().unwrap();
            if let geojson::Value::Polygon(polygon) = &geometry1.value {
                assert_eq!(polygon.len(), 1);
                assert_eq!(polygon[0].len(), 23);
            } else {
                panic!("Expected a Polygon geometry for feature1");
            }
            let expected_path: &Path = Path::new("tests/data/input1.las");

            assert_eq!(
                feature1
                    .properties
                    .as_ref()
                    .unwrap()
                    .get("filename")
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
                assert_eq!(polygon[0].len(), 30); // Adjust the number of points as needed
            } else {
                panic!("Expected a Polygon geometry for feature2");
            }
            let expected_path: &Path = Path::new("tests/data/input2.las");
            assert_eq!(
                feature2
                    .properties
                    .as_ref()
                    .unwrap()
                    .get("filename")
                    .unwrap()
                    .as_str()
                    .map(Path::new),
                Some(expected_path)
            );
        } else {
            panic!("Expected a FeatureCollection");
        }

        // Cleanup: Remove the output file
        fs::remove_file(output_path).expect("Failed to delete the output file");
    }

    #[test]

    fn test_integration_workflow_group_by_folder() {
        let folder_path = "tests/data";
        let result = process_folder(folder_path, true, true, true);
        assert!(result.is_ok());

        // Check if the output file is created
        let output_path = Path::new("las_outlines.geojson");
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

            let expected_path = Path::new("tests/data");
            let folder_path = properties.get("folder_path").unwrap().as_str().unwrap();
            assert_eq!(Path::new(folder_path), expected_path);

            // Check the geometry of the first feature
            assert!(feature.geometry.is_some());
            let geometry = feature.geometry.as_ref().unwrap();
            if let Value::Polygon(coords) = &geometry.value {
                // Check the number of coordinate sets (should be 1 for a simple polygon)
                assert_eq!(coords.len(), 1);

                // Check the number of coordinates in the exterior ring
                let exterior_ring = &coords[0];
                assert_eq!(exterior_ring.len(), 25);

                // Check specific coordinates (e.g., the first and last)
                assert_eq!(exterior_ring[0], vec![1771069.242, 5917200.036]);
                assert_eq!(exterior_ring[24], vec![1771069.242, 5917200.036]);
            } else {
                panic!("Expected Polygon geometry");
            }
        } else {
            panic!("Expected FeatureCollection");
        }

        // Clean up
        fs::remove_file(output_path).unwrap();
    }
}
