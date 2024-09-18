use std::env;
use std::process;

fn main() {
    // Collect the command-line arguments
    let args: Vec<String> = env::args().collect();

    // Check if the file path argument is provided
    if args.len() < 2 {
        eprintln!("Usage: {} <path_to_las_folder>", args[0]);
        process::exit(1);
    }

    let folder_path = &args[1];

    // Call the function from lib.rs
    if let Err(e) = las_poly::process_folder(folder_path, true, true) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
