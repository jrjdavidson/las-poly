use geo::{ConvexHull, Coord, LineString, Polygon};
use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};
use serde_json::Map;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

pub struct LasOutlineFeatureCollection {
    features: Vec<Feature>,
}

impl LasOutlineFeatureCollection {
    pub fn new() -> Self {
        Self {
            features: Vec::new(),
        }
    }

    pub fn add_feature(&mut self, feature: Feature) {
        self.features.push(feature);
    }

    pub fn save_to_file(&self, output_file_name: &str) -> std::io::Result<()> {
        let feature_collection = FeatureCollection {
            features: self.features.clone(),
            bbox: None,
            foreign_members: None,
        };

        let geojson = GeoJson::FeatureCollection(feature_collection);
        let mut file = File::create(output_file_name)?;
        file.write_all(geojson.to_string().as_bytes())?;
        println!("Merged polygons saved to {}", output_file_name);
        Ok(())
    }

    pub fn merge_geometries(&mut self) {
        let mut features_by_folder: HashMap<String, Vec<Geometry>> = HashMap::new();
        for feature in self.features.drain(..) {
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

            self.add_feature(feature);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geojson::Value;
    use serde_json::json;
    use std::fs;

    #[test]
    fn test_new_las_feature_collection() {
        let collection = LasOutlineFeatureCollection::new();
        assert!(collection.features.is_empty());
    }

    #[test]
    fn test_add_feature() {
        let mut collection = LasOutlineFeatureCollection::new();
        let feature = Feature {
            geometry: Some(Geometry::new(Value::Point(vec![1.0, 2.0]))),
            properties: Some(Map::new()),
            id: None,
            bbox: None,
            foreign_members: None,
        };
        collection.add_feature(feature);
        assert_eq!(collection.features.len(), 1);
    }

    #[test]
    fn test_save_to_file() {
        let mut collection = LasOutlineFeatureCollection::new();
        let mut properties = Map::new();
        properties.insert("SourceFileDir".to_string(), json!("folder1"));
        properties.insert("Attribute1".to_string(), json!("Value1"));
        properties.insert("Attribute2".to_string(), json!(42));

        let feature = Feature {
            geometry: Some(Geometry::new(Value::Point(vec![1.0, 2.0]))),
            properties: Some(properties.clone()),
            id: None,
            bbox: None,
            foreign_members: None,
        };
        collection.add_feature(feature);

        let output_file_name = "test_output.geojson";
        collection.save_to_file(output_file_name).unwrap();

        let saved_content = fs::read_to_string(output_file_name).unwrap();
        let geojson: GeoJson = saved_content.parse().unwrap();
        if let GeoJson::FeatureCollection(fc) = geojson {
            assert_eq!(fc.features.len(), 1);
            let saved_feature = &fc.features[0];
            if let Some(saved_properties) = &saved_feature.properties {
                assert_eq!(saved_properties.get("SourceFileDir").unwrap(), "folder1");
                assert_eq!(saved_properties.get("Attribute1").unwrap(), "Value1");
                assert_eq!(saved_properties.get("Attribute2").unwrap(), 42);
            } else {
                panic!("Expected properties");
            }
        } else {
            panic!("Expected a FeatureCollection");
        }

        fs::remove_file(output_file_name).unwrap();
    }

    #[test]
    fn test_merge_geometries() {
        let mut collection = LasOutlineFeatureCollection::new();
        let mut properties = Map::new();
        properties.insert("SourceFileDir".to_string(), json!("folder1"));
        properties.insert("Attribute1".to_string(), json!("Value1"));
        properties.insert("number_of_points".to_string(), json!(42));
        let feature1 = Feature {
            geometry: Some(Geometry::new(Value::Polygon(vec![vec![
                vec![0.0, 0.0],
                vec![1.0, 0.0],
                vec![1.0, 1.0],
                vec![0.0, 1.0],
                vec![0.0, 0.0],
            ]]))),
            properties: Some(properties.clone()),
            id: None,
            bbox: None,
            foreign_members: None,
        };

        let feature2 = Feature {
            geometry: Some(Geometry::new(Value::Polygon(vec![vec![
                vec![1.0, 1.0],
                vec![2.0, 1.0],
                vec![2.0, 2.0],
                vec![1.0, 2.0],
                vec![1.0, 1.0],
            ]]))),
            properties: Some(properties.clone()),
            id: None,
            bbox: None,
            foreign_members: None,
        };

        collection.add_feature(feature1);
        collection.add_feature(feature2);
        collection.merge_geometries();

        assert_eq!(collection.features.len(), 1);
        let merged_feature = &collection.features[0];
        if let Some(geometry) = &merged_feature.geometry {
            if let Value::Polygon(coords) = &geometry.value {
                assert_eq!(coords[0].len(), 7); // Convex hull should have 8 points
            } else {
                panic!("Expected a Polygon");
            }
        } else {
            panic!("Expected a geometry");
        }
        if let Some(properties) = &merged_feature.properties {
            let source_file_dir = properties.get("SourceFileDir").unwrap().as_str().unwrap();
            assert_eq!(source_file_dir, "folder1");
            assert!(properties.get("Attribute1").is_none());
            assert!(properties.get("number_of_points").is_none());
        } else {
            panic!("Expected properties");
        }
    }
}
