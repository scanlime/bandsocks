use std::process::Command;
use std::path::Path;
use build_deps::rerun_if_changed_paths;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let sand_crate = Path::new("../sand");
    let sand_target = Path::new(&out_dir).join("sand-target");
    
    rerun_if_changed_paths( sand_crate.join("**").to_str().unwrap() ).unwrap();
    assert!(Command::new("cargo")
            .current_dir(sand_crate)
            .arg("build")
            .arg("--release")
            .arg("--target-dir").arg(sand_target)
            .status().unwrap().success())
}
