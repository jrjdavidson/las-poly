use geo::{ConvexHull, Coord, LineString, Polygon};
use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};
use serde_json::Map;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

pub struct LasOutlineFeatureCollection {
    features: Vec<Feature>,
}

type FolderFeatures = (Vec<Geometry>, u64, HashMap<String, Vec<String>>);

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

    pub fn merge_geometries(&mut self, only_join_if_shared_vertex: bool) {
        let mut features_by_folder: HashMap<String, FolderFeatures> = HashMap::new();

        self.group_features_by_folder(&mut features_by_folder);

        for (folder_path, (geometries, total_points, other_properties)) in features_by_folder {
            if only_join_if_shared_vertex {
                let groups = self.group_by_shared_vertex(geometries);
                for geometries in groups {
                    let merged_polygon = self.merge_group(geometries);
                    self.create_feature(
                        folder_path.clone(),
                        total_points,
                        other_properties.clone(),
                        merged_polygon,
                    );
                }
            } else {
                let merged_polygon = self.merge_group(geometries);
                self.create_feature(folder_path, total_points, other_properties, merged_polygon);
            };
        }
    }

    fn group_features_by_folder(
        &mut self,
        features_by_folder: &mut HashMap<String, FolderFeatures>,
    ) {
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
            let number_of_points: u64 = feature
                .properties
                .as_ref()
                .unwrap()
                .get("number_of_points")
                .unwrap()
                .as_u64()
                .unwrap();
            let mut other_properties = HashMap::new();
            for (key, value) in feature.properties.as_ref().unwrap().iter() {
                if key != "SourceFileDir" && key != "number_of_points" {
                    if let Some(value_str) = value.as_str() {
                        other_properties
                            .entry(key.clone())
                            .or_insert_with(Vec::new)
                            .push(value_str.to_string());
                    }
                }
            }

            features_by_folder
                .entry(folder_path.clone())
                .or_default()
                .0
                .push(geometry);
            features_by_folder.entry(folder_path.clone()).or_default().1 += number_of_points;
            for (key, values) in other_properties {
                let entry = features_by_folder
                    .entry(folder_path.clone())
                    .or_insert_with(|| (Vec::new(), 0, HashMap::new()))
                    .2
                    .entry(key)
                    .or_default();
                for value in values {
                    if !entry.contains(&value) {
                        entry.push(value);
                    }
                }
            }
        }
    }

    fn group_by_shared_vertex(&self, mut geometries: Vec<Geometry>) -> Vec<Vec<Geometry>> {
        let mut groups: Vec<Vec<Geometry>> = vec![];

        while let Some(geometry) = geometries.pop() {
            let mut group = vec![geometry];
            let mut i = 0;
            while i < geometries.len() {
                let geom_coords = if let Value::Polygon(coords) = &geometries[i].value {
                    coords[0]
                        .iter()
                        .map(|c| Coord { x: c[0], y: c[1] })
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };

                let shared_vertex = group.iter().any(|g| {
                    if let Value::Polygon(group_coords) = &g.value {
                        let group_coords: Vec<Coord<f64>> = group_coords[0]
                            .iter()
                            .map(|c| Coord { x: c[0], y: c[1] })
                            .collect();
                        group_coords.iter().any(|c| geom_coords.contains(c))
                    } else {
                        false
                    }
                });

                if shared_vertex {
                    group.push(geometries.remove(i));
                } else {
                    i += 1;
                }
            }
            groups.push(group);
        }

        groups
    }

    fn merge_group(&self, geometries: Vec<Geometry>) -> Polygon<f64> {
        geometries.into_iter().fold(
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
        )
    }

    fn create_feature(
        &mut self,
        folder_path: String,
        total_points: u64,
        other_properties: HashMap<String, Vec<String>>,
        merged_polygon: Polygon<f64>,
    ) {
        let exterior_coords: Vec<Vec<f64>> = merged_polygon
            .exterior()
            .coords()
            .map(|c| vec![c.x, c.y])
            .collect();
        let geojson_polygon = Value::Polygon(vec![exterior_coords]);
        let geometry = Geometry::new(geojson_polygon);
        let mut properties = Map::new();
        properties.insert("SourceFileDir".to_string(), folder_path.into());
        properties.insert("number_of_points".to_string(), total_points.into());
        for (key, values) in &other_properties {
            properties.insert(
                key.to_string(),
                serde_json::Value::Array(
                    values
                        .iter()
                        .map(|v| serde_json::Value::String(v.clone()))
                        .collect(),
                ),
            );
        }
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
        let mut properties3 = properties.clone();
        properties3.insert("Attribute1".to_string(), json!("Value2"));
        properties3.insert("Attribute2".to_string(), json!("Value3"));
        properties3.insert("Attribute3".to_string(), json!("!@#$%^&*()"));
        let feature3 = Feature {
            geometry: Some(Geometry::new(Value::Polygon(vec![vec![
                vec![1.0, 1.0],
                vec![2.0, 1.0],
                vec![2.0, 2.0],
                vec![1.0, 2.0],
                vec![1.0, 1.0],
            ]]))),
            properties: Some(properties3.clone()),
            id: None,
            bbox: None,
            foreign_members: None,
        };

        collection.add_feature(feature1);
        collection.add_feature(feature2);
        collection.add_feature(feature3);
        collection.merge_geometries(false);

        assert_eq!(collection.features.len(), 1);
        let merged_feature = &collection.features[0];
        if let Some(geometry) = &merged_feature.geometry {
            if let Value::Polygon(coords) = &geometry.value {
                assert_eq!(coords[0].len(), 7); // Convex hull should have 7 points
            } else {
                panic!("Expected a Polygon");
            }
        } else {
            panic!("Expected a geometry");
        }
        if let Some(properties) = &merged_feature.properties {
            let source_file_dir = properties.get("SourceFileDir").unwrap().as_str().unwrap();
            assert_eq!(source_file_dir, "folder1");

            let number_of_points = properties
                .get("number_of_points")
                .unwrap()
                .as_u64()
                .unwrap();
            assert_eq!(number_of_points, 126);

            let attribute1 = properties.get("Attribute1").unwrap().as_array().unwrap();
            assert_eq!(attribute1, &vec![json!("Value1"), json!("Value2")]);

            let attribute2 = properties.get("Attribute2").unwrap().as_array().unwrap();
            assert_eq!(attribute2, &vec![json!("Value3")]);

            let attribute3 = properties.get("Attribute3").unwrap().as_array().unwrap();
            assert_eq!(attribute3, &vec![json!("!@#$%^&*()")]);
        } else {
            panic!("Expected properties");
        }
    }

    #[test]
    fn test_merge_geometries_with_shared_vertex() {
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
        let feature3 = Feature {
            geometry: Some(Geometry::new(Value::Polygon(vec![vec![
                vec![0.0, 2.0],
                vec![2.0, 3.0],
                vec![4.0, 5.0],
                vec![6.0, 7.0],
                vec![8.0, 9.0],
            ]]))),
            properties: Some(properties.clone()),
            id: None,
            bbox: None,
            foreign_members: None,
        };

        collection.add_feature(feature1);
        collection.add_feature(feature2);
        collection.add_feature(feature3);
        println!("{:?}", collection.features);

        collection.merge_geometries(true);

        assert_eq!(collection.features.len(), 2);
        let merged_feature1 = &collection.features[0];
        let merged_feature2 = &collection.features[1];
        println!("{:?}", merged_feature1);
        println!("{:?}", merged_feature2);
        if let Some(geometry) = &merged_feature1.geometry {
            if let Value::Polygon(coords) = &geometry.value {
                assert_eq!(coords[0].len(), 4);
                println!("{:?}", coords);
            } else {
                panic!("Expected a Polygon");
            }
        } else {
            panic!("Expected a geometry");
        }
        if let Some(geometry) = &merged_feature2.geometry {
            if let Value::Polygon(coords) = &geometry.value {
                assert_eq!(coords[0].len(), 7);
                println!("{:?}", coords);
            } else {
                panic!("Expected a Polygon");
            }
        } else {
            panic!("Expected a geometry");
        }
    }

    #[test]
    fn test_merge_geometries_without_shared_vertex() {
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
                vec![2.0, 2.0],
                vec![3.0, 2.0],
                vec![3.0, 3.0],
                vec![2.0, 3.0],
                vec![2.0, 2.0],
            ]]))),
            properties: Some(properties.clone()),
            id: None,
            bbox: None,
            foreign_members: None,
        };

        collection.add_feature(feature1);
        collection.add_feature(feature2);
        collection.merge_geometries(true);

        assert_eq!(collection.features.len(), 2);
    }
}
