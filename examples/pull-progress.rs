use bandsocks::{ImageName, Pull, PullProgress, RegistryClient};

#[tokio::main]
async fn main() {
    env_logger::init();
    let name = "ubuntu@sha256:a569d854594dae4c70f0efef5f5857eaa3b97cdb1649ce596b113408a0ad5f7f";
    let name: ImageName = name.parse().unwrap();
    let client = RegistryClient::builder();
    let client = client.ephemeral_cache().build().unwrap();
    show_progress(&mut client.pull_progress(&name)).await;
    show_progress(&mut client.pull_progress(&name)).await;
}

async fn show_progress(pull: &mut Pull) {
    loop {
        match pull.progress().await {
            PullProgress::Done(result) => {
                println!("done: {:?}", result.unwrap());
                break;
            }
            PullProgress::Update(update) => {
                println!("update: {:?}", update);
            }
        }
    }
}
