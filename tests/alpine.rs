use bandsocks::{Container, ContainerBuilder};
use tokio::{runtime::Runtime};

const IMAGE: &str =
    "alpine:latest";

async fn pull() -> ContainerBuilder {
    Container::pull(&IMAGE.parse().unwrap())
        .await
        .expect("container pull")
}

#[test]
fn alpine_true() {
    Runtime::new().unwrap().block_on(async {
        let container = pull().await.arg("/bin/true").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(-1));
    })
}
