use std::error::Error;

use las::{Point, Reader};
use rand::Rng;

#[derive(Debug)]
pub enum Crs {
    Wkt(String),
    GeoTiff(Vec<u8>),
}

pub fn extract_crs(file_path: &str) -> Result<Option<Crs>, Box<dyn Error>> {
    let reader = Reader::from_path(file_path)?;

    let header = reader.header();

    // Check if the CRS is WKT
    if header.has_wkt_crs() {
        // Look for WKT records in VLRs and EVLRs
        if let Some(crs) = header
            .vlrs()
            .iter()
            .chain(header.evlrs().iter())
            .find_map(|vlr| match vlr.user_id.as_str() {
                "LASF_Projection" => match vlr.record_id {
                    2111 | 2112 => Some(Crs::Wkt(String::from_utf8_lossy(&vlr.data).to_string())),
                    _ => None,
                },
                _ => None,
            })
        {
            return Ok(Some(crs));
        }
    } else {
        // Look for GeoTIFF records in VLRs only
        if let Some(crs) = header
            .vlrs()
            .iter()
            .find_map(|vlr| match vlr.user_id.as_str() {
                "LASF_Projection" => match vlr.record_id {
                    34735..=34737 => Some(Crs::GeoTiff(vlr.data.clone())),
                    _ => None,
                },
                _ => None,
            })
        {
            return Ok(Some(crs));
        }
    }
    // If no CRS information is found, attempt to guess CRS from point data
    println!(
        "No CRS found in VLRs data, attempting to guess CRS from a random sample of 10 points",
    );
    let points = grab_random_points(reader, 10)?;
    if let Some(guessed_crs) = guess_crs_from_points(points) {
        return Ok(Some(guessed_crs));
    }

    Ok(None)
}
fn grab_random_points(mut reader: Reader, num_points: usize) -> Result<Vec<Point>, Box<dyn Error>> {
    let total_points = reader.header().number_of_points();
    let mut rng = rand::thread_rng();
    let mut points = Vec::with_capacity(num_points);

    for _ in 0..num_points {
        let random_index = rng.gen_range(0..total_points);
        reader.seek(random_index)?;
        if let Some(point) = reader.read_point()? {
            points.push(point);
        }
    }

    Ok(points)
}
fn guess_crs_from_points(points: Vec<Point>) -> Option<Crs> {
    if points.is_empty() {
        return None;
    }

    // Check if all points are within the bounds of EPSG:4326
    if points
        .iter()
        .all(|point| point.x > -180.0 && point.x < 180.0 && point.y > -90.0 && point.y < 90.0)
    {
        return Some(Crs::Wkt("EPSG:4326".to_string()));
    };

    // Check if all points are within the bounds of EPSG:2193
    if points.iter().all(|point| {
        point.x > 800000.0 && point.x < 2400000.0 && point.y > 4000000.0 && point.y < 9000000.0
    }) {
        return Some(Crs::Wkt("EPSG:2193".to_string()));
    }

    None
}

pub fn extract_crs_from_geotiff(data: &[u8]) -> Result<String, Box<dyn Error>> {
    // Parse the GeoTIFF data to extract CRS information
    // This is a simplified example, you may need to use a GeoTIFF parsing library for full implementation
    let geotiff_string = String::from_utf8_lossy(data).to_string();
    Ok(geotiff_string)
}
