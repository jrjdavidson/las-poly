use geojson::GeoJson;
use geojson::{Feature, Geometry, Value};
use las_poly::las_feature_collection::LasOutlineFeatureCollection;
use serde_json::json;
use serde_json::Map;
use std::fs;

#[test]
fn test_new_las_feature_collection() {
    let collection = LasOutlineFeatureCollection::new();
    assert!(collection.features().is_empty());
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
    assert_eq!(collection.features().len(), 1);
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
    collection.merge_geometries(false, false);

    assert_eq!(collection.features().len(), 1);
    let merged_feature = &collection.features()[0];
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
        assert!(attribute1.contains(&json!("Value1")));
        assert!(attribute1.contains(&json!("Value2")));

        let attribute2 = properties.get("Attribute2").unwrap().as_array().unwrap();
        assert!(attribute2.contains(&json!("Value3")));

        let attribute3 = properties.get("Attribute3").unwrap().as_array().unwrap();
        assert!(attribute3.contains(&json!("!@#$%^&*()")));
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

    collection.merge_geometries(true, false);

    assert_eq!(collection.features().len(), 2);
    let merged_features = collection.features();
    let merged_feature1 = merged_features
        .iter()
        .find(|f| {
            if let Some(geometry) = &f.geometry {
                if let Value::Polygon(coords) = &geometry.value {
                    coords[0].len() == 4
                } else {
                    false
                }
            } else {
                false
            }
        })
        .expect("Expected a merged feature with 4 points");

    let merged_feature2 = merged_features
        .iter()
        .find(|f| {
            if let Some(geometry) = &f.geometry {
                if let Value::Polygon(coords) = &geometry.value {
                    coords[0].len() == 7
                } else {
                    false
                }
            } else {
                false
            }
        })
        .expect("Expected a merged feature with 7 points");

    if let Some(geometry) = &merged_feature1.geometry {
        if let Value::Polygon(coords) = &geometry.value {
            assert_eq!(coords[0].len(), 4);
        } else {
            panic!("Expected a Polygon");
        }
    } else {
        panic!("Expected a geometry");
    }
    if let Some(geometry) = &merged_feature2.geometry {
        if let Value::Polygon(coords) = &geometry.value {
            assert_eq!(coords[0].len(), 7);
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
    collection.merge_geometries(true, false);

    assert_eq!(collection.features().len(), 2);
}

#[test]
fn test_merge_geometries_with_overlap() {
    let mut collection = LasOutlineFeatureCollection::new();
    let mut properties = Map::new();
    properties.insert("SourceFileDir".to_string(), json!("folder1"));
    properties.insert("Attribute1".to_string(), json!("Value1"));
    properties.insert("number_of_points".to_string(), json!(42));
    let feature1 = Feature {
        geometry: Some(Geometry::new(Value::Polygon(vec![vec![
            vec![0.0, 0.0],
            vec![2.0, 0.0],
            vec![2.0, 2.0],
            vec![0.0, 2.0],
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
            vec![3.0, 1.0],
            vec![3.0, 3.0],
            vec![1.0, 3.0],
            vec![1.0, 1.0],
        ]]))),
        properties: Some(properties.clone()),
        id: None,
        bbox: None,
        foreign_members: None,
    };

    collection.add_feature(feature1);
    collection.add_feature(feature2);
    collection.merge_geometries(false, true);

    assert_eq!(collection.features().len(), 1);
    let merged_feature = &collection.features()[0];
    if let Some(geometry) = &merged_feature.geometry {
        if let Value::Polygon(coords) = &geometry.value {
            assert_eq!(coords[0].len(), 7); // Convex hull should have 8 points
        } else {
            panic!("Expected a Polygon");
        }
    } else {
        panic!("Expected a geometry");
    }
}

#[test]
fn test_merge_geometries_with_shared_vertex_and_overlap() {
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
            vec![0.5, 0.5],
            vec![1.5, 0.5],
            vec![1.5, 1.5],
            vec![0.5, 1.5],
            vec![0.5, 0.5],
        ]]))),
        properties: Some(properties.clone()),
        id: None,
        bbox: None,
        foreign_members: None,
    };

    collection.add_feature(feature1);
    collection.add_feature(feature2);
    collection.add_feature(feature3);
    collection.merge_geometries(true, true);

    assert_eq!(collection.features().len(), 1);
    let merged_feature = &collection.features()[0];
    if let Some(geometry) = &merged_feature.geometry {
        if let Value::Polygon(coords) = &geometry.value {
            assert_eq!(coords[0].len(), 7); // Convex hull should have 8 points
        } else {
            panic!("Expected a Polygon");
        }
    } else {
        panic!("Expected a geometry");
    }
}
