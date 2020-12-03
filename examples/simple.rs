#[tokio::main]
async fn main() {
    env_logger::init();
    let name = "busybox@sha256:cddb0e8f24f292e9b7baaba4d5f546db08f0a4b900be2048c6bd704bd90c13df";
    bandsocks::Container::pull(&name.parse().unwrap())
        .await
        .unwrap()
        .arg("busybox")
        .arg("--help")
        .interact()
        .await
        .unwrap();
}
