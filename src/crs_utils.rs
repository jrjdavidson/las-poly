use las::{Point, Reader};
use rand::Rng;
use thiserror::Error;

#[derive(Debug, PartialEq)]
pub enum Crs {
    Wkt(String),
    GeoTiff(Vec<u8>),
}

#[derive(Error, Debug)]
pub enum CrsError {
    #[error("Failed to read LAS file: {0}")]
    Las(#[from] las::Error),
    #[error("Failed to parse GeoTIFF data: {0}")]
    GeoTiff(String),
    #[error("Failed to guess CRS from points")]
    Guess,
}

pub fn extract_crs(file_path: &str) -> Result<Option<Crs>, CrsError> {
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

fn grab_random_points(mut reader: Reader, num_points: usize) -> Result<Vec<Point>, CrsError> {
    let total_points = reader.header().number_of_points();
    let num_points = num_points.min(total_points as usize); // Use the minimum between total_points and num_points
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

pub fn extract_crs_from_geotiff(data: &[u8]) -> Result<String, CrsError> {
    // Parse the GeoTIFF data to extract CRS information
    // This is a simplified example, you may need to use a GeoTIFF parsing library for full implementation
    let geotiff_string = String::from_utf8_lossy(data).to_string();
    Ok(geotiff_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use las::{Header, Point, Writer};
    use std::fs;
    use std::fs::File;
    use std::io::Write;

    #[test]
    #[ignore]
    fn test_extract_crs_geotiff() {
        // Create a mock LAS file with GeoTIFF CRS
        let file_path = "tests/data/mock_geotiff.las";
        let header = Header::default();
        {
            let mut writer = Writer::from_path(file_path, header).unwrap();
            writer.write_point(Default::default()).unwrap(); // Write an empty point record
        }
        // Add GeoTIFF CRS to the header
        let mut file = File::open(file_path).unwrap();
        file.write_all(b"LASF_Projection34735GeoTIFFData").unwrap();

        let crs = extract_crs(file_path).unwrap();
        assert!(matches!(crs, Some(Crs::GeoTiff(_))));
        assert_eq!(crs.unwrap(), Crs::GeoTiff(b"GeoTIFFData".to_vec()));

        // Clean up
        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_extract_crs_guess_epsg4326() {
        // Create a mock LAS file with points in EPSG:4326 bounds
        let file_path = "tests/data/mock_epsg4326.las";
        let header = Header::default();
        let mut writer = Writer::from_path(file_path, header).unwrap();
        let points = vec![
            Point {
                x: 10.0,
                y: 20.0,
                z: 30.0,
                ..Default::default()
            },
            Point {
                x: -10.0,
                y: -20.0,
                z: -30.0,
                ..Default::default()
            },
        ];
        for point in points {
            writer.write_point(point).unwrap();
        }
        writer.close().unwrap();
        let crs = extract_crs(file_path).unwrap();
        assert!(matches!(crs, Some(Crs::Wkt(_))));
        assert_eq!(crs.unwrap(), Crs::Wkt("EPSG:4326".to_string()));

        // Clean up
        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_extract_crs_none() {
        // Create a mock LAS file with no CRS information
        let file_path = "tests/data/mock_none.las";
        let header = Header::default();
        let mut writer = Writer::from_path(file_path, header).unwrap();
        writer
            .write_point(Point {
                x: 1000.0,
                y: 5000.0,
                z: 30.0,
                ..Default::default()
            })
            .unwrap(); // Write an empty point record
        writer.close().unwrap();
        let crs = extract_crs(file_path).unwrap();
        assert!(crs.is_none());

        // Clean up
        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_extract_crs_from_geotiff() {
        let data = b"GeoTIFFData";
        let crs = extract_crs_from_geotiff(data).unwrap();
        assert_eq!(crs, "GeoTIFFData".to_string());
    }

    #[test]
    fn test_extract_crs_from_wkt() {
        use proj::Proj;

        // Test for VLRs data in the specified LAS file
        let file_path = "tests/crs/BQ29_1000_4907.las";
        let crs = extract_crs(file_path).unwrap();
        assert!(crs.is_some());

        if let Some(Crs::Wkt(wkt)) = crs {
            assert!(!wkt.is_empty());

            // Check if proj accepts the WKT
            let proj = Proj::new(wkt.trim_end_matches(char::from(0)));
            println!("{:?}", proj);
            assert!(proj.is_ok());
        } else if let Some(Crs::GeoTiff(data)) = crs {
            println!("CRS found (GeoTIFF): {:?}", data);
            assert!(!data.is_empty());

            // Check if proj accepts the GeoTIFF data
            let crs_string = extract_crs_from_geotiff(&data).unwrap();
            let proj = Proj::new(&crs_string);
            assert!(proj.is_ok());
        } else {
            panic!("Expected CRS information in VLRs");
        }
    }
    #[test]
    fn test_extract_crs_from_geoTifff() {
        use proj::Proj;

        // Test for VLRs data in the specified LAS file
        let file_path = "tests/crs/Rolleston_SecondSortie.las";
        let crs = extract_crs(file_path).unwrap();
        assert!(crs.is_some());

        if let Some(Crs::Wkt(wkt)) = crs {
            assert!(!wkt.is_empty());

            // Check if proj accepts the WKT
            let proj = Proj::new(wkt.trim_end_matches(char::from(0)));
            println!("{:?}", proj);
            assert!(proj.is_ok());
        } else if let Some(Crs::GeoTiff(data)) = crs {
            println!("CRS found (GeoTIFF): {:?}", data);
            assert!(!data.is_empty());

            // Check if proj accepts the GeoTIFF data
            let crs_string = extract_crs_from_geotiff(&data).unwrap();
            let proj = Proj::new(&crs_string.trim_end_matches(char::from(0)));
            println!("{:?}", proj);
            assert!(proj.is_ok());
        } else {
            panic!("Expected CRS information in VLRs");
        }
    }
}
