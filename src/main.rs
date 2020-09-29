// This code may not be used for any purpose. Be gay, do crime.

#[macro_use]
extern crate clap;

use clap::{App, ArgMatches};
use std::error::Error;
use env_logger::{Env, from_env};
use bandsocks_runtime::{Reference, CacheBuilder};

fn main() -> Result<(), Box<dyn Error>> {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let log_level = matches.value_of("log_level").unwrap();
    from_env(Env::default().default_filter_or(log_level)).init();

    let run_args = string_values(&matches, "run_args");
    let image_reference: Reference = matches.value_of("image_reference").unwrap().parse()?;
    let cache = CacheBuilder::new().build();
   
    println!("{:?} {:?}", image_reference, run_args);

    let result = cache.pull(&image_reference);

    println!("{:?}", result);

    Ok(())
}

fn string_values<S: AsRef<str>>(matches: &ArgMatches, name: S) -> Vec<String> {
    match matches.values_of(name) {
        Some(strs) => strs.map(|s| s.to_string()).collect(),
        None => Vec::new(),
    }
}
