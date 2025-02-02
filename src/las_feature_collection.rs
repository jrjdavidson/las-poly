use geo::{ConvexHull, Coord, Intersects, LineString, Polygon};
use geojson::{Feature, FeatureCollection, GeoJson, Geometry, JsonObject, Value};
use log::{debug, info};
use std::fs::File;
use std::io::Write;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};
use union_find::{QuickUnionUf, UnionByRank, UnionFind};

const EPSILON: f64 = 1e-7;

pub struct LasOutlineFeatureCollection {
    features: Vec<Feature>,
}

struct OrderedCoord {
    x: f64,
    y: f64,
}

impl PartialEq for OrderedCoord {
    fn eq(&self, other: &Self) -> bool {
        (self.x - other.x).abs() < EPSILON && (self.y - other.y).abs() < EPSILON
    }
}

impl Eq for OrderedCoord {}

impl Hash for OrderedCoord {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let x_hash = (self.x / EPSILON).round() as i64;
        let y_hash = (self.y / EPSILON).round() as i64;
        x_hash.hash(state);
        y_hash.hash(state);
    }
}

impl LasOutlineFeatureCollection {
    pub fn new() -> Self {
        Self {
            features: Vec::new(),
        }
    }
    pub fn features(&self) -> &Vec<Feature> {
        &self.features
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
        info!("Merged polygons saved to {}", output_file_name);
        Ok(())
    }

    pub fn merge_geometries(&mut self, only_join_if_shared_vertex: bool, merge_if_overlap: bool) {
        let features_by_folder = self.group_features_by_folder();
        for (folder_path, features) in features_by_folder {
            if only_join_if_shared_vertex || merge_if_overlap {
                let groups = self.group_by_shared_vertex(&features);
                if merge_if_overlap {
                    let mut shared_features = Vec::new();
                    for group in groups {
                        let merged_feature_opt = self.merge_group(group, &folder_path);
                        if let Some(merged_feature) = merged_feature_opt {
                            shared_features.push(merged_feature);
                        }
                    }
                    let merged_group = self.group_by_overlap(&shared_features);
                    for group in merged_group {
                        let merged_feature_opt = self.merge_group(group, &folder_path);
                        if let Some(merged_feature) = merged_feature_opt {
                            self.add_feature(merged_feature);
                        }
                    }
                } else {
                    for group in groups {
                        let merged_feature_opt = self.merge_group(group, &folder_path);
                        if let Some(merged_feature) = merged_feature_opt {
                            self.add_feature(merged_feature);
                        }
                    }
                }
            } else {
                let merged_feature_opt = self.merge_group(features, &folder_path);
                if let Some(merged_feature) = merged_feature_opt {
                    self.add_feature(merged_feature);
                }
            }
        }
    }

    pub fn group_features_by_folder(&mut self) -> HashMap<String, Vec<Feature>> {
        let mut folder_map: HashMap<String, Vec<Feature>> = HashMap::new();

        for feature in self.features.drain(..) {
            let folder_name = feature
                .properties
                .as_ref()
                .unwrap()
                .get("SourceFileDir")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            folder_map
                .entry(folder_name)
                .or_default()
                .push(feature.clone());
        }

        folder_map
    }
    // {  for feature in self.features.drain(..) {
    //         let folder_path = feature
    //             .properties
    //             .as_ref()
    //             .unwrap()
    //             .get("SourceFileDir")
    //             .unwrap()
    //             .as_str()
    //             .unwrap()
    //             .to_string();
    //         let geometry = feature.geometry.unwrap();
    //         let number_of_points: u64 = feature
    //             .properties
    //             .as_ref()
    //             .unwrap()
    //             .get("number_of_points")
    //             .unwrap()
    //             .as_u64()
    //             .unwrap();
    //         let mut other_properties = HashMap::new();
    //         for (key, value) in feature.properties.as_ref().unwrap().iter() {
    //             if key != "SourceFileDir" && key != "SourceFile" && key != "number_of_points" {
    //                 if let Some(value_str) = value.as_str() {
    //                     other_properties
    //                         .entry(key.clone())
    //                         .or_insert_with(Vec::new)
    //                         .push(value_str.to_string());
    //                 }
    //             }
    //         }

    //         features_by_folder
    //             .entry(folder_path.clone())
    //             .or_default()
    //             .0
    //             .push(geometry);
    //         features_by_folder.entry(folder_path.clone()).or_default().1 += number_of_points;
    //         for (key, values) in other_properties {
    //             let entry = features_by_folder
    //                 .entry(folder_path.clone())
    //                 .or_insert_with(|| (Vec::new(), 0, HashMap::new()))
    //                 .2
    //                 .entry(key)
    //                 .or_default();
    //             for value in values {
    //                 if !entry.contains(&value) {
    //                     entry.push(value);
    //                 }
    //             }
    //         }
    //     }
    // }

    fn group_by_shared_vertex(&self, features: &[Feature]) -> Vec<Vec<Feature>> {
        let mut vertex_to_index: HashMap<OrderedCoord, Vec<usize>> = HashMap::new();
        let mut uf = QuickUnionUf::<UnionByRank>::new(features.len());

        for (i, feature) in features.iter().enumerate() {
            if let Some(Geometry {
                value: Value::Polygon(coords),
                ..
            }) = &feature.geometry
            {
                for coord in &coords[0] {
                    let ordered_coord = OrderedCoord {
                        x: coord[0],
                        y: coord[1],
                    };
                    if let Some(indices) = vertex_to_index.get(&ordered_coord) {
                        for &index in indices {
                            uf.union(i, index);
                        }
                    }
                    vertex_to_index.entry(ordered_coord).or_default().push(i);
                }
            }
        }

        let mut groups: HashMap<usize, Vec<Feature>> = HashMap::new();
        for (i, feature) in features.iter().enumerate() {
            let root = uf.find(i);
            groups.entry(root).or_default().push(feature.clone());
        }

        groups.into_values().collect()
    }

    fn group_by_overlap(&self, features: &[Feature]) -> Vec<Vec<Feature>> {
        let mut uf = QuickUnionUf::<UnionByRank>::new(features.len());

        for i in 0..features.len() {
            for j in (i + 1)..features.len() {
                if let (Value::Polygon(coords1), Value::Polygon(coords2)) =
                    if let (Some(geom1), Some(geom2)) =
                        (&features[i].geometry, &features[j].geometry)
                    {
                        (&geom1.value, &geom2.value)
                    } else {
                        continue;
                    }
                {
                    let poly1 = Polygon::new(
                        LineString::from(
                            coords1[0]
                                .iter()
                                .map(|c| Coord { x: c[0], y: c[1] })
                                .collect::<Vec<_>>(),
                        ),
                        vec![],
                    );
                    let poly2 = Polygon::new(
                        LineString::from(
                            coords2[0]
                                .iter()
                                .map(|c| Coord { x: c[0], y: c[1] })
                                .collect::<Vec<_>>(),
                        ),
                        vec![],
                    );
                    if poly1.intersects(&poly2) {
                        uf.union(i, j);
                    }
                }
            }
        }

        let mut groups: HashMap<usize, Vec<Feature>> = HashMap::new();
        for (i, feature) in features.iter().enumerate() {
            let root = uf.find(i);
            groups.entry(root).or_default().push(feature.clone());
        }

        groups.into_values().collect()
    }

    fn merge_group(&self, features: Vec<Feature>, folder_path: &String) -> Option<Feature> {
        let merged_polygon = features.iter().fold(
            Polygon::new(LineString::new(vec![]), vec![]),
            |acc, feature| {
                if let Some(Geometry {
                    value: Value::Polygon(geom_coords),
                    ..
                }) = &feature.geometry
                {
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

        // Log a warning if the merged polygon has fewer than 4 points

        if merged_polygon.exterior().coords().count() < 4 {
            info!(
                "Merged polygon has fewer than 4 points: {:?}",
                merged_polygon.exterior().coords().collect::<Vec<_>>()
            );
            return None;
        }
        // Merge properties
        let mut merged_properties: JsonObject = JsonObject::new();
        merged_properties.insert(
            "SourceFileDir".to_string(),
            serde_json::Value::String(folder_path.to_string()),
        );
        for feature in features {
            merged_properties
                .entry("number_of_features".to_string())
                .and_modify(|e| {
                    if let serde_json::Value::Number(n) = e {
                        if let Some(count) = n.as_u64() {
                            *e = serde_json::Value::Number(serde_json::Number::from(count + 1));
                        }
                    }
                })
                .or_insert_with(|| serde_json::Value::Number(serde_json::Number::from(1)));
            if let Some(properties) = &feature.properties {
                if let Some(number_of_points_value) = properties.get("number_of_points") {
                    if let Some(number_of_points) = number_of_points_value.as_u64() {
                        merged_properties
                            .entry("number_of_points".to_string())
                            .and_modify(|e| {
                                if let serde_json::Value::Number(n) = e {
                                    if let Some(count) = n.as_u64() {
                                        *e = serde_json::Value::Number(serde_json::Number::from(
                                            count + number_of_points,
                                        ));
                                    }
                                }
                            })
                            .or_insert_with(|| {
                                serde_json::Value::Number(serde_json::Number::from(
                                    number_of_points,
                                ))
                            });
                    }
                }
                for (key, value) in properties.iter() {
                    if key != "SourceFile" && key != "SourceFileDir" && key != "number_of_points" {
                        match value {
                            serde_json::Value::String(value_str) => {
                                insert_unique_value(
                                    &mut merged_properties,
                                    key,
                                    serde_json::Value::String(value_str.clone()),
                                );
                            }
                            serde_json::Value::Number(value_num) => {
                                insert_unique_value(
                                    &mut merged_properties,
                                    key,
                                    serde_json::Value::Number(value_num.clone()),
                                );
                            }
                            serde_json::Value::Array(value_arr) => {
                                for item in value_arr {
                                    insert_unique_value(&mut merged_properties, key, item.clone());
                                }
                            }
                            _ => {
                                debug!("Unhandled format for key/value pair {:?}:{:?}", key, value);
                            }
                        }
                    }
                }
            }
        }
        // Create a feature with the merged polygon and properties
        Some(Feature {
            geometry: Some(Geometry {
                value: Value::Polygon(vec![merged_polygon
                    .exterior()
                    .coords()
                    .map(|c| vec![c.x, c.y])
                    .collect()]),
                bbox: None,
                foreign_members: None,
            }),
            properties: Some(merged_properties),
            ..Default::default()
        })
    }
}

impl Default for LasOutlineFeatureCollection {
    fn default() -> Self {
        LasOutlineFeatureCollection::new()
    }
}

fn insert_unique_value(
    merged_properties: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: serde_json::Value,
) {
    let entry = merged_properties
        .entry(key.to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));

    if let serde_json::Value::Array(arr) = entry {
        if !arr.iter().any(|v| v == &value) {
            arr.push(value);
        }
    }
}
