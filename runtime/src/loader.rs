// This code may not be used for any purpose. Be gay, do crime.

use pentacle::SealedCommand;
use std::io::Cursor;
use std::os::unix::process::CommandExt;

pub fn do_the_thing() {

    const ELF: &'static [u8] = include_bytes!(concat!(
        env!("OUT_DIR"), "/sand-target/release/bandsocks-sand"));

    let mut elf_reader = Cursor::new(ELF);
    let mut cmd = SealedCommand::new(&mut elf_reader).unwrap();
    cmd.arg0("sand");
    cmd.arg("just_some_strings");
    cmd.arg("where_a_blob_of_config_data_will_go");

    println!("{:?}", cmd.status());
}

