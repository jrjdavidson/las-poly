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

    /// Use a detailed outline. The default simple outline uses the header information for the data bounds, this option will read every point and create a convex hull around points.
    #[arg(short, long)]
    use_detailed_outline: bool,

    /// Group by folder - create one polygon outline per folder.
    #[arg(short, long)]
    group_by_folder: bool,

    /// Recurse into subfolders
    #[arg(short, long)]
    recurse: bool,
}

fn main() {
    let args = Args::parse();
    if let Err(e) = las_poly::process_folder(
        &args.folder_path,
        args.use_detailed_outline,
        args.group_by_folder,
        args.recurse,
    ) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
