use bandsocks::{container::ContainerBuilder, Container};
use tokio::runtime::Runtime;

const IMAGE: &str = "gcr.io/google-samples/hello-app:1.0";

async fn common() -> ContainerBuilder {
    let _ = env_logger::builder().is_test(true).try_init();
    Container::pull(&IMAGE.parse().unwrap())
        .await
        .expect("container pull")
}

#[test]
fn pull() {
    Runtime::new().unwrap().block_on(async {
        common().await;
    })
}

/*
#[test]
fn gcr_hello() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}
*/
