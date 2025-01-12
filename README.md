# las_poly

A command-line tool for processing LAS files and generating GeoJSON polygons.

## Overview

`las_poly` processes folders containing LAS files, generates polygons from the LAS data, and saves the results as a GeoJSON file. It supports options for detailed outlines, grouping by folder, and recursion into subdirectories.

## Features

- **Detailed Outlines**: Option to read every point and create a convex hull around points for a detailed outline.
- **Grouping by Folder**: Create one polygon outline per folder.
- **Recursion**: Recurse into subdirectories to process LAS files.

## Installation

A working proj crate is required. vcpkg was tested on windows, [using method here](https://github.com/georust/proj/pull/79#issuecomment-1308751602). Required also adding the dll directory to the env variable path,for example, if you installed vcpkg in C:\src\vcpkg, the DLLs should be in C:\src\vcpkg\installed\x64-windows\bin.

Clone this repository and use it as a library, or as command line tool by building the project. 

## Usage
```
las_poly <folder_path> [<output_name>] [--use_detailed_outline] [--group_by_folder] [--recurse]
```
### Examples
Process a folder with detailed outlines, grouping by folder, and recursion:
```
las_poly "path/to/folder" --use_detailed_outline --group_by_folder --recurse
```
## Command-line Arguments
--folder_path: Path to the folder containing LAS files.
--output_name: Name of the output file. If not present, name will be the folder name.
--use_detailed_outline: Use a detailed outline. The default simple outline uses the header information for the data bounds.
--group_by_folder: Group by folder - create one polygon outline per folder.
--recurse: Recurse into subfolders.
--guess_crs: Attempt to guess crs from a random sample of 10 points.
## Contributing
Contributions are welcome! Please open an issue or submit a pull request.

## Author
Jonathan Davidson - jrjdavidson@gmail.com