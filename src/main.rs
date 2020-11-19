#[macro_use] extern crate clap;

use bandsocks::Container;
use clap::{App, ArgMatches};
use env_logger::{from_env, Env};

#[tokio::main]
async fn main() {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let log_level = matches.value_of("log_level").unwrap();
    from_env(Env::default().default_filter_or(log_level)).init();

    let run_args = string_values(&matches, "run_args");
    let run_env = env_values(&matches, "run_env");
    let image_reference = matches
        .value_of("image_reference")
        .unwrap()
        .parse()
        .expect("bad image reference");

    Container::pull(&image_reference)
        .await
        .expect("failed to pull container image")
        .args(run_args)
        .envs(run_env)
        .spawn()
        .expect("container failed to start")
        .wait()
        .await
        .expect("failed waiting for container to stop")
}

fn string_values<S: AsRef<str>>(matches: &ArgMatches, name: S) -> Vec<String> {
    matches
        .values_of(name)
        .into_iter()
        .map(|values| values.map(|value| value.to_string()))
        .flatten()
        .collect()
}

fn env_values<S: AsRef<str>>(matches: &ArgMatches, name: S) -> Vec<(String, String)> {
    string_values(matches, name)
        .iter()
        .map(|env_str| {
            let mut parts = env_str.splitn(2, '=');
            (
                parts.next().unwrap().to_string(),
                parts.next().unwrap_or("").to_string(),
            )
        })
        .collect()
}
