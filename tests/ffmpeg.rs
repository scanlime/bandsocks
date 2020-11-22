use bandsocks::{container::ContainerBuilder, Container};
use tokio::runtime::Runtime;

const IMAGE: &str =
    "jrottenberg/ffmpeg@sha256:fd2d5fb9f4f18aaf0b568f153d9042be115df626d9cbe7920b8b9063ca654b2a";

async fn pull() -> ContainerBuilder {
    Container::pull(&IMAGE.parse().unwrap())
        .await
        .expect("container pull")
}

#[test]
fn ffmpeg_help() {
    env_logger::init();
    Runtime::new().unwrap().block_on(async {
        let container = pull().await.spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}
