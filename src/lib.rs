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
//!     use std::fs;
//!     use tempfile::tempdir;
//!
//!     let temp_dir = tempdir()?;
//!     let test_folder = temp_dir.path().join("test_folder");
//!     fs::create_dir_all(&test_folder)?;
//!     process_folder(test_folder.to_str().unwrap(),true, true, true, true,true, None)?;
//!     // Cleanup: Remove the file created in the root if it exists
//!     let output_file = "test_folder.geojson";
//!     if fs::metadata(output_file).is_ok() {
//!         fs::remove_file(output_file)?;
//!     }
//!
//!     Ok(())
//! }
//! ```

mod crs_utils;
mod las_feature_collection;

use crs_utils::{extract_crs, extract_crs_from_geotiff, Crs, CrsError};
use geo::{ConvexHull, Coord, LineString, Polygon};
use las::Reader;
use serde::Serialize;
use serde_json::Map;

use std::path::Path;
use std::sync::mpsc;
use std::thread;

use thiserror::Error;
use threadpool::ThreadPool;
use walkdir::WalkDir;

use geojson::Feature;
use geojson::{Geometry, Value};
use las_feature_collection::LasOutlineFeatureCollection;

/// Processes a folder containing LAS files and generates GeoJSON polygons.
///
/// # Arguments
///
/// * `folder_path` - The path to the folder containing LAS files.
/// * `use_detailed_outline` - Whether to use detailed outlines for the polygons.
/// * `group_by_folder` - Whether to group the polygons by folder.
/// * `recurse` - Whether to recurse into subdirectories.
/// * `guess_crs` - Whether to guess the crs based on a random sample of 10 points.
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
///     use std::fs;
///     use tempfile::tempdir;
///
///     let temp_dir = tempdir()?;
///     let test_folder = temp_dir.path().join("test_folder");
///
///     fs::create_dir_all(&test_folder)?;
///    let output_path = temp_dir.path().join("output.geojson").to_str().unwrap().to_string();
///     process_folder(test_folder.to_str().unwrap(),true, true, true, false,true, Some(&output_path))?;
///     Ok(())
/// }
/// ```
#[derive(Error, Debug)]
pub enum LasPolyError {
    #[error("Failed to read LAS file: {0}")]
    LasError(#[from] las::Error),
    #[error("Failed to transform coordinates: {0}")]
    ProjError(#[from] proj::ProjError),
    #[error("Failed to extract CRS: {0}")]
    CrsError(#[from] CrsError),
    #[error("Failed to create output file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Unable to find path{0}")]
    PathError(String),
    #[error("Failed to create Proj instance: {0}")]
    ProjCreateError(#[from] proj::ProjCreateError),
}
pub fn process_folder(
    folder_path: &str,
    use_detailed_outline: bool,
    group_by_folder: bool,
    merge_if_shared_vertex: bool,
    recurse: bool,
    guess_crs: bool,
    output_file: Option<&str>,
) -> Result<(), LasPolyError> {
    let path = Path::new(folder_path);

    // Check if the folder exists
    if !path.exists() {
        return Err(LasPolyError::PathError(folder_path.to_string()));
    }
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

            match create_polygon(&file_path, use_detailed_outline, guess_crs) {
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

    let mut feature_collection = LasOutlineFeatureCollection::new();

    // Collect features from the channel
    for feature in feature_rx {
        feature_collection.add_feature(feature);
    }

    // Merge geometries if group_by_folder is true
    if group_by_folder || merge_if_shared_vertex {
        feature_collection.merge_geometries(merge_if_shared_vertex);
    }

    let path = std::path::Path::new(folder_path);
    let file_stem = path
        .file_name()
        .unwrap_or_else(|| path.components().last().unwrap().as_os_str());
    let binding = format!("{}.geojson", file_stem.to_string_lossy());
    let output_file_name = output_file.unwrap_or(&binding);

    feature_collection.save_to_file(output_file_name)?;

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
///     let feature = create_polygon("tests/data/input1.las", true, true)?;
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
    guess_crs: bool,
) -> Result<Feature, LasPolyError> {
    // Open the LAS file
    let mut crs = match extract_crs(file_path, guess_crs)? {
        // Check the CRS of the LAS file
        Some(Crs::Wkt(wkt)) => Some(wkt),
        Some(Crs::GeoTiff(geo_key_directory, geo_double_params, geo_ascii_params)) => {
            Some(extract_crs_from_geotiff(
                &geo_key_directory,
                geo_double_params.as_deref(),
                geo_ascii_params.as_deref(),
            )?)
        }
        None => {
            println!("No CRS found for {}. Will not add data.", file_path);
            None
        }
    };
    if crs.is_none() {
        return Err(LasPolyError::CrsError(CrsError::MissingCrs));
    };
    crs = Some(crs.unwrap().trim_end_matches(char::from(0)).to_string());
    // Create a Proj instance for transforming coordinates to EPSG:4326
    let to_epsg4326 =
        Proj::new_known_crs(&crs.unwrap(), "EPSG:4326", None).map_err(LasPolyError::from)?;
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
