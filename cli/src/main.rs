#[macro_use] extern crate clap;

use bandsocks::{
    Container, Image, ImageError, ProgressEvent, ProgressPhase, ProgressResource, Pull,
    PullProgress, RegistryClient,
};
use clap::{App, ArgMatches};
use env_logger::{from_env, Env};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{collections::HashMap, path::Path, sync::Arc};
use tokio::task;

#[tokio::main]
async fn main() {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).version(crate_version!()).get_matches();

    let log_level = matches.value_of("log_level").unwrap();
    from_env(Env::default().default_filter_or(log_level)).init();

    let run_args = string_values(&matches, "run_args");
    let run_env = env_values(&matches, "run_env");
    let image_reference = matches
        .value_of("image_reference")
        .unwrap()
        .parse()
        .expect("bad image reference");

    let mut client = RegistryClient::builder();
    if let Some(dir) = matches.value_of("cache_dir") {
        client = client.cache_dir(Path::new(dir));
    }
    if matches.is_present("ephemeral") {
        client = client.ephemeral_cache();
    }
    if matches.is_present("offline") {
        client = client.offline();
    }
    let client = client.build().unwrap();

    let image = (if matches.is_present("quiet") {
        client.pull(&image_reference).await
    } else {
        show_pull_progress(client.pull_progress(&image_reference)).await
    })
    .expect("failed to pull container image");

    if matches.is_present("pull") {
        if !run_args.is_empty() || !run_env.is_empty() {
            log::warn!("pull-only mode, run arguments are being ignored")
        }
    } else {
        let mut container = Container::new(image)
            .expect("failed to construct container")
            .args(run_args)
            .envs(run_env);

        if matches.is_present("entrypoint") {
            container = container.entrypoint(string_values(&matches, "entrypoint"));
        }
        if matches.is_present("instruction_trace") {
            container = container.instruction_trace();
        }
        let container = container.spawn().expect("container failed to start");

        match container.interact().await {
            Ok(status) => {
                if let Some(code) = status.code() {
                    std::process::exit(code);
                }
            }
            Err(err) => {
                log::error!("{}", err);
                std::process::exit(0xFF);
            }
        }
    }
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

async fn show_pull_progress(mut pull: Pull) -> Result<Arc<Image>, ImageError> {
    const TEMPLATE: &str =
        "{percent:>3}% {prefix:10} {spinner} {wide_msg}  [{bar:25}] {bytes:>9}/{total_bytes:>9}";
    const TICK_CHARS: &str = "/-\\| ";

    let multi = Arc::new(MultiProgress::new());
    let task_multi = multi.clone();
    let task_progress = multi.add(ProgressBar::new_spinner());
    let task_join = task::spawn(async move {
        let mut bars: HashMap<Arc<ProgressResource>, ProgressBar> = HashMap::new();
        loop {
            match pull.progress().await {
                PullProgress::Done(result) => {
                    task_progress.finish();
                    return result;
                }
                PullProgress::Update(progress) => {
                    let bar = match bars.get(&progress.resource) {
                        Some(bar) => bar,
                        None => {
                            let new_bar = task_multi.add(match progress.event {
                                ProgressEvent::Begin => ProgressBar::new_spinner(),
                                ProgressEvent::BeginSized(s) => ProgressBar::new(s),
                                ProgressEvent::Progress(_) | ProgressEvent::Complete => continue,
                            });
                            new_bar.set_message(&progress.resource.to_string());
                            bars.insert(progress.resource.clone(), new_bar);
                            bars.get(&progress.resource).unwrap()
                        }
                    };

                    match progress.event {
                        ProgressEvent::Begin | ProgressEvent::BeginSized(_) => {
                            bar.reset();
                            bar.set_prefix(match progress.phase {
                                ProgressPhase::Connect => "connect",
                                ProgressPhase::Download => "download",
                                ProgressPhase::Decompress => "decompress",
                            });
                            bar.set_style(
                                ProgressStyle::default_bar()
                                    .template(TEMPLATE)
                                    .tick_chars(TICK_CHARS)
                                    .progress_chars(match progress.phase {
                                        ProgressPhase::Connect => "  ",
                                        ProgressPhase::Download => "- ",
                                        ProgressPhase::Decompress => "=-",
                                    }),
                            );
                        }
                        ProgressEvent::Progress(_) | ProgressEvent::Complete => {}
                    }

                    match progress.event {
                        ProgressEvent::Begin => {
                            bar.enable_steady_tick(250);
                            bar.set_length(1);
                        }
                        ProgressEvent::BeginSized(s) => {
                            bar.disable_steady_tick();
                            bar.set_length(s);
                        }
                        ProgressEvent::Progress(p) => bar.set_position(p),
                        ProgressEvent::Complete => bar.finish(),
                    }
                }
            }
        }
    });
    task::spawn_blocking(move || multi.join()).await??;
    task_join.await?
}
