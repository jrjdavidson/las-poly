use las::{Point, Reader};
use rand::Rng;
use thiserror::Error;

#[derive(Debug, PartialEq)]
pub enum Crs {
    Wkt(String),
    GeoTiff(Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>), // Store all three tags
}

#[derive(Error, Debug)]
pub enum CrsError {
    #[error("Failed to read LAS file: {0}")]
    Las(#[from] las::Error),
    #[error("Failed to parse GeoTIFF data: {0}")]
    GeoTiff(String),
    #[error("Failed to guess CRS from points")]
    Guess,
    #[error("Failed to create Proj instance: {0}")]
    DecoderError(String),
    #[error("Failed to read GeoKeyDirectoryTag: {0}")]
    GeoKeyDirectoryTagError(String),
    #[error("CRS information not found in GeoTIFF")]
    CrsNotFoundError,
    #[error("CRS information not found in file")]
    MissingCrs,
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
        let mut geo_key_directory_tag = None;
        let mut geo_double_params_tag = None;
        let mut geo_ascii_params_tag = None;

        for vlr in header.vlrs().iter() {
            if vlr.user_id.as_str() == "LASF_Projection" {
                match vlr.record_id {
                    34735 => geo_key_directory_tag = Some(vlr.data.clone()),
                    34736 => geo_double_params_tag = Some(vlr.data.clone()),
                    34737 => geo_ascii_params_tag = Some(vlr.data.clone()),
                    _ => {}
                }
            }
        }

        if let Some(geo_key_directory) = geo_key_directory_tag {
            let crs = Crs::GeoTiff(
                geo_key_directory,
                geo_double_params_tag,
                geo_ascii_params_tag,
            );
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

pub fn extract_crs_from_geotiff(
    geo_key_directory: &[u8],
    geo_double_params: Option<&[u8]>,
    geo_ascii_params: Option<&[u8]>,
) -> Result<String, CrsError> {
    // Parse the GeoKeyDirectoryTag

    let geo_key_directory_tag: Vec<u16> = geo_key_directory
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    println!("{:?}", geo_key_directory_tag);
    let mut proj_string = String::new();
    let num_keys = geo_key_directory_tag[3] as usize;
    for i in 0..num_keys {
        let key_id = geo_key_directory_tag[4 + i * 4];
        let tiff_tag_location = geo_key_directory_tag[5 + i * 4];
        let count = geo_key_directory_tag[6 + i * 4];
        let value_offset = geo_key_directory_tag[7 + i * 4];

        match key_id {
            2048 => {
                // GeographicTypeGeoKey
                if value_offset != 32767 {
                    proj_string = format!("EPSG:{} ", value_offset);
                }
            }
            3072 => {
                // ProjectedCSTypeGeoKey
                if value_offset != 32767 {
                    proj_string = format!("EPSG:{} ", value_offset);
                }
            }

            1026 => {
                if tiff_tag_location == 34736 {
                    if let Some(geo_double_params) = geo_double_params {
                        let value = geo_double_params[value_offset as usize];
                        proj_string = format!("{}", value);
                    }
                } else if tiff_tag_location == 34737 {
                    if let Some(geo_ascii_params) = geo_ascii_params {
                        let value = &geo_ascii_params
                            [value_offset as usize..(value_offset + count - 1) as usize];
                        proj_string = String::from_utf8_lossy(value).to_string();
                    }
                }
            }
            _ => {}
        }
    }

    Ok(proj_string.trim().to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use las::{Header, Point, Writer};
    use std::fs;

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
        } else {
            panic!("Expected CRS information in VLRs");
        }
    }
    #[test]
    fn test_extract_crs_from_geo_tiff() {
        use proj::Proj;

        // Test for VLRs data in the specified LAS file
        let file_path = "tests/crs/merged.las";
        let crs = extract_crs(file_path).unwrap();
        assert!(crs.is_some());

        if let Some(Crs::GeoTiff(data1, data2, data3)) = crs {
            println!("CRS found (GeoTIFF): {:?}", data1);
            assert!(!data1.is_empty());

            // Check if proj accepts the GeoTIFF data
            let crs_string =
                extract_crs_from_geotiff(&data1, data2.as_deref(), data3.as_deref()).unwrap();
            println!("{:?}", crs_string);
            let proj = Proj::new_known_crs(&crs_string, "EPSG:4326", None);
            println!("{:?}", proj);
            assert!(proj.is_ok());
        } else {
            panic!("Expected CRS information in VLRs");
        }
    }
    #[test]

    fn test_unexpected_crs() {
        // Test for VLRs data in the specified LAS file
        let file_path = "tests/crs/BLK002598.las";
        let crs = extract_crs(file_path).unwrap();
        println!("{:?}", crs);
        assert!(crs.is_none());
    }
}
