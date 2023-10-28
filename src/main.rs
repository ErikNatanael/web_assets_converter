use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Command,
};

use clap::Parser;
use color_eyre::eyre::{Result, *};
use walkdir::WalkDir;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the folder with original assets
    #[arg(short, long, default_value = "./")]
    asset_path: String,
    /// Path to the destination folder
    #[arg(short, long, default_value = "../dist/assets/")]
    destination_path: String,
    /// The maximum file size of copied non-image files in MiB
    #[arg(short, long, default_value_t = 20)]
    max_file_size: u64,
    /// If false, files that already exist will not be reencoded
    #[arg(short, long, default_value_t = false)]
    clean: bool,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = Args::parse();
    println!("Processing files in {}", args.asset_path);
    let asset_path = std::fs::canonicalize(PathBuf::from(&args.asset_path)).unwrap();
    if !asset_path.is_dir() {
        panic!("Asset path is not a directory: {}", asset_path.display());
    }
    if asset_path.file_stem().unwrap().to_string_lossy() != "assets" {
        println!(
            "Program not started in a directory called \"assets\", do you want to continue? [y/N]"
        );
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if answer != "y" && answer != "Y" {
            return Ok(());
        }
    }
    for entry in WalkDir::new(&args.asset_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if match entry.path().extension().and_then(OsStr::to_str) {
            Some("jpg" | "JPG" | "png" | "PNG" | "jpeg") => {
                println!("{}", entry.path().display());
                // convert to a smaller file size
                convert_image(entry.path(), &args)?;
                // If the original is large, also convert to a 4k file size
                // Also convert to a thumbnail file size
                true
            }
            _ => false,
        } {
            // File was handled
        } else {
            if entry.path().is_file() {
                // File was not handled based on its extension
                let file_size = entry.path().metadata().unwrap().len();
                const MIB: u64 = 2_u64.pow(20);
                if file_size < args.max_file_size * MIB {
                    // Copy it over
                    if let Err(e) = copy_file_as_is(entry.path(), &args) {
                        eprintln!("Error: {:?}", e);
                    }
                }
            }
        }
    }
    Ok(())
}

fn get_destination_path(source: &Path, args: &Args) -> Result<PathBuf> {
    let relative_file = source.strip_prefix(&args.asset_path)?;
    let mut new_path = PathBuf::from(&args.destination_path);
    new_path.push(relative_file);
    Ok(new_path)
}

fn copy_file_as_is(file: &Path, args: &Args) -> Result<()> {
    let relative_file = file.strip_prefix(&args.asset_path)?;
    println!("Copying {}", relative_file.display());
    let mut new_path = PathBuf::from(&args.destination_path);
    new_path.push(relative_file);
    if new_path == *file {
        // Copying a file to itself can lead to corruption
        return Err(eyre!(
            "source and destination paths are the same: {}",
            file.display()
        ));
    }
    if let Some(p) = new_path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::copy(&file, &new_path).wrap_err_with(|| {
        format!(
            "source: {}, destination: {}",
            file.display(),
            new_path.display()
        )
    })?;
    Ok(())
}

fn convert_image(source_path: &Path, args: &Args) -> Result<()> {
    let mut destination_path = get_destination_path(source_path, &args)?;
    let is_png = destination_path
        .extension()
        .unwrap()
        .to_string_lossy()
        .to_lowercase()
        == "png";
    if is_png {
        destination_path.set_extension("jpg");
    }
    if let Some(p) = destination_path.parent() {
        std::fs::create_dir_all(p)?;
    }
    // Create normal quality default version
    if args.clean || !destination_path.exists() {
        Command::new("convert")
            .arg(source_path)
            .arg("-strip")
            .arg("-interlace")
            .arg("Plane")
            .arg("-gaussian-blur")
            .arg("0.05")
            .arg("-quality")
            .arg("85%")
            .arg("-resize")
            .arg("1920x1920")
            .arg(&destination_path)
            .output()?;
    }
    if destination_path.metadata().unwrap().len() > source_path.metadata().unwrap().len() {
        // This particular file is smaller as its original size than as a downsized jpg so use the original image
        let destination_path = get_destination_path(source_path, &args)?;
        std::fs::copy(&source_path, &destination_path).wrap_err_with(|| {
            format!(
                "source: {}, destination: {}",
                source_path.display(),
                destination_path.display()
            )
        })?;
    }
    // Check if it's worth creating a higher res version
    {
        let mut destination_path = destination_path.clone();
        let org_file_name = destination_path.file_stem().unwrap().to_string_lossy();
        let org_extension = destination_path.extension().unwrap().to_string_lossy();
        destination_path.set_file_name(format!("{org_file_name}_high.{org_extension}"));
        println!("high_path: {destination_path:?}");
        if args.clean || !destination_path.exists() {
            // let img = image::open(source_path)?;
            // if img.width() >= 3840 || img.height() >= 3840 {
            Command::new("convert")
                .arg(source_path)
                .arg("-strip")
                .arg("-interlace")
                .arg("Plane")
                // .arg("-gaussian-blur")
                // .arg("0.02")
                .arg("-quality")
                .arg("85%")
                .arg("-resize")
                .arg("3840x3840")
                .arg(&destination_path)
                .output()?;
            // Sometimes the resulting file is larger than the original. In that case, copy the original to the new destination instead.
            if destination_path.metadata().unwrap().len() > source_path.metadata().unwrap().len() {
                std::fs::copy(&source_path, &destination_path).wrap_err_with(|| {
                    format!(
                        "source: {}, destination: {}",
                        source_path.display(),
                        destination_path.display()
                    )
                })?;
            }
            // }
        }
    }
    // Create a thumbnail version
    let mut destination_path = destination_path.clone();
    let org_file_name = destination_path.file_stem().unwrap().to_string_lossy();
    let org_extension = destination_path.extension().unwrap().to_string_lossy();
    destination_path.set_file_name(format!("{org_file_name}_thumb.{org_extension}"));
    println!("thumb_path: {destination_path:?}");
    if args.clean || !destination_path.exists() {
        Command::new("convert")
            .arg(source_path)
            .arg("-strip")
            .arg("-interlace")
            .arg("Plane")
            .arg("-gaussian-blur")
            .arg("0.01")
            .arg("-quality")
            .arg("85%")
            .arg("-resize")
            .arg("640x640")
            .arg(&destination_path)
            .output()?;
    }
    Ok(())

    // convert "$f" \
    // -strip \
    // -interlace Plane \
    // -gaussian-blur 0.05 \
    // -quality 85% \
    // -resize 1920x1920\> \
    // "$f"
}
