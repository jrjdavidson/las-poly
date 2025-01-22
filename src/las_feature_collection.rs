use geo::{ConvexHull, Coord, Intersects, LineString, Polygon};
use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};
use log::info;
use serde_json::Map;
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

type FolderFeatures = (Vec<Geometry>, u64, HashMap<String, Vec<String>>);

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
        let mut features_by_folder: HashMap<String, FolderFeatures> = HashMap::new();
        self.group_features_by_folder(&mut features_by_folder);

        for (folder_path, (geometries, total_points, other_properties)) in features_by_folder {
            if only_join_if_shared_vertex || merge_if_overlap {
                let groups = self.group_by_shared_vertex(&geometries);
                if merge_if_overlap {
                    let mut shared_geometries = Vec::new();
                    for group in groups {
                        let merged_polygon = self.merge_group(group);
                        let exterior_coords: Vec<Vec<f64>> = merged_polygon
                            .exterior()
                            .coords()
                            .map(|c| vec![c.x, c.y])
                            .collect();
                        let geometry = Geometry::new(Value::Polygon(vec![exterior_coords]));
                        shared_geometries.push(geometry);
                    }

                    let merged_group = self.group_by_overlap(&shared_geometries);
                    for group in merged_group {
                        let merged_polygon = self.merge_group(group);
                        self.create_feature(
                            folder_path.clone(),
                            total_points,
                            other_properties.clone(),
                            merged_polygon,
                        );
                    }
                } else {
                    for group in groups {
                        let merged_polygon = self.merge_group(group);
                        self.create_feature(
                            folder_path.clone(),
                            total_points,
                            other_properties.clone(),
                            merged_polygon,
                        );
                    }
                }
            } else {
                let merged_polygon = self.merge_group(geometries);
                self.create_feature(folder_path, total_points, other_properties, merged_polygon);
            }
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
                if key != "SourceFileDir" && key != "SourceFile" && key != "number_of_points" {
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

    fn group_by_shared_vertex(&self, geometries: &[Geometry]) -> Vec<Vec<Geometry>> {
        let mut vertex_to_index: HashMap<OrderedCoord, Vec<usize>> = HashMap::new();
        let mut uf = QuickUnionUf::<UnionByRank>::new(geometries.len());

        for (i, geometry) in geometries.iter().enumerate() {
            if let Value::Polygon(coords) = &geometry.value {
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

        let mut groups: HashMap<usize, Vec<Geometry>> = HashMap::new();
        for (i, geometry) in geometries.iter().enumerate() {
            let root = uf.find(i);
            groups.entry(root).or_default().push(geometry.clone());
        }

        groups.into_values().collect()
    }

    fn group_by_overlap(&self, geometries: &[Geometry]) -> Vec<Vec<Geometry>> {
        let mut uf = QuickUnionUf::<UnionByRank>::new(geometries.len());

        for i in 0..geometries.len() {
            for j in (i + 1)..geometries.len() {
                if let (Value::Polygon(coords1), Value::Polygon(coords2)) =
                    (&geometries[i].value, &geometries[j].value)
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

        let mut groups: HashMap<usize, Vec<Geometry>> = HashMap::new();
        for (i, geometry) in geometries.iter().enumerate() {
            let root = uf.find(i);
            groups.entry(root).or_default().push(geometry.clone());
        }

        groups.into_values().collect()
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

impl Default for LasOutlineFeatureCollection {
    fn default() -> Self {
        LasOutlineFeatureCollection::new()
    }
}
