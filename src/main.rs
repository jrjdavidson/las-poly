//! A command-line tool for processing LAS files and generating GeoJSON polygons.
//!
//! This tool processes folders containing LAS files, generates polygons from the LAS data,
//! and saves the results as a GeoJSON file. It supports options for detailed outlines, grouping
//! by folder, and recursion into subdirectories.
//!
//! # Usage
//!
//! ```sh
//! las_poly --folder_path <path> [--use_detailed_outline] [--group_by_folder] [--recurse] [--guess_crs]
//! ```
//!
//! # Examples
//!
//! ```sh
//! las_poly --folder_path "path/to/folder" --use_detailed_outline --group_by_folder --recurse
//! ```
use clap::Parser;
use std::process;

/// Command-line arguments structure
#[derive(Parser)]
#[command(
    name = "las_poly",
    version = "1.0",
    author = "Jonathan Davidson <jrjddavidson@gmail.com>",
    about = "Creates a geojson file with the outlines of LAS files found in the specified folder"
)]
struct Args {
    /// Path to the folder containing LAS files
    folder_path: String,
    name: Option<String>,

    /// Use a detailed outline. The default simple outline uses the header information for the data bounds, this option will read every point and create a convex hull around points.
    #[arg(short, long)]
    use_detailed_outline: bool,

    /// Group by folder - create one polygon outline per folder.
    #[arg(long)]
    group_by_folder: bool,

    /// Merge Tiled - only merges outlines if polygons shares a vertex.
    #[arg(short, long)]
    merge_tiled: bool,

    /// Recurse into subfolders
    #[arg(short, long)]
    recurse: bool,

    /// Guess the CRS of the las file is the WKT or Geotiff header information is not present.
    #[arg(short, long)]
    guess_crs: bool,
}

fn main() {
    let args = Args::parse();
    if let Err(e) = las_poly::process_folder(
        &args.folder_path,
        args.use_detailed_outline,
        args.group_by_folder,
        args.merge_tiled,
        args.recurse,
        args.guess_crs,
        args.name.as_deref(),
    ) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
