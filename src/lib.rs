use geo::ConvexHull;
use geo::{Coord, LineString, Polygon};
use las::Reader;
use std::error::Error;
use std::sync::mpsc;
use std::thread;
use threadpool::ThreadPool;
use walkdir::WalkDir;

use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};
use std::fs::File;
use std::io::Write;

pub fn process_folder(folder_path: &str, use_simple_outline: bool) -> Result<(), Box<dyn Error>> {
    let num_threads = num_cpus::get();
    println!("Number of threads used: {:?}", num_cpus);

    let pool = ThreadPool::new(num_threads);
    let (tx, rx) = mpsc::channel();

    // Spawn a thread to walk through the directory and send file paths
    let folder_path = folder_path.to_string();
    thread::spawn(move || {
        for entry in WalkDir::new(folder_path).into_iter().filter_map(Result::ok) {
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

            if let Ok(feature) = create_polygon(&file_path, use_simple_outline) {
                feature_tx.send(feature).unwrap();
                println!("{:?} polygon successfully created", file_path);
            } else {
                println!("Error in thread {:?}", file_path);
            }
        });
    }

    drop(feature_tx); // Close the channel to signal completion

    // Collect features from the channel
    let mut features = Vec::new();
    for feature in feature_rx {
        features.push(feature);
    }

    // Create a FeatureCollection from all the features
    let feature_collection = FeatureCollection {
        features,
        bbox: None,
        foreign_members: None,
    };
    let geojson = GeoJson::FeatureCollection(feature_collection);

    // Save the GeoJSON to a file
    let mut file = File::create("polygons.geojson")?;
    file.write_all(geojson.to_string().as_bytes())?;

    println!("Polygons saved to polygons.geojson");

    Ok(())
}

fn create_polygon(file_path: &str, use_simple_outline: bool) -> Result<Feature, Box<dyn Error>> {
    // Open the LAS file
    let mut reader = Reader::from_path(file_path)?;

    let geojson_polygon = if use_simple_outline {
        // Use the header to create a simple outline
        let header = reader.header();
        let exterior_coords = vec![
            vec![header.min_x, header.min_y],
            vec![header.max_x, header.min_y],
            vec![header.max_x, header.max_y],
            vec![header.min_x, header.max_y],
            vec![header.min_x, header.min_y],
        ];
        Value::Polygon(vec![exterior_coords])
    } else {
        // Collect points
        let points: Vec<Coord<f64>> = reader
            .points()
            .filter_map(Result::ok)
            .map(|p| Coord { x: p.x, y: p.y })
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
    let feature = Feature {
        geometry: Some(geometry),
        properties: None,
        id: None,
        bbox: None,
        foreign_members: None,
    };

    Ok(feature)
}
