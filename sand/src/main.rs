// This code may not be used for any purpose. Be gay, do crime.

use std::process::Command;

fn main() {
    println!("sand");
    Command::new("/bin/sh").status().unwrap();
}
