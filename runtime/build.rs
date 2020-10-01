// This code may not be used for any purpose. Be gay, do crime.

use std::process::Command;
use std::path::Path;
use build_deps::rerun_if_changed_paths;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let sand_target = Path::new(&out_dir).join("sand-target");

    rerun_if_changed_paths("../sand/*").unwrap();
    rerun_if_changed_paths("../sand/src/*").unwrap();   

    assert!(Command::new("cargo")
            .current_dir("../sand")
            .arg("+nightly")
            .arg("build")
            .arg("--release")
            .arg("--target-dir").arg(sand_target)
            .status().unwrap().success())
}
