use bandsocks::{container::ContainerBuilder, Container};
use tokio::runtime::Runtime;

const IMAGE: &str =
    "gcr.io/google-samples/hello-app:1.0";

async fn pull() -> ContainerBuilder {
    Container::pull(&IMAGE.parse().unwrap())
        .await
        .expect("container pull")
}

#[test]
fn gcr_hello() {
    env_logger::init();
    Runtime::new().unwrap().block_on(async {
        let container = pull().await.spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}
