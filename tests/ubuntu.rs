use bandsocks::{Container, ContainerBuilder};
use tokio::runtime::Runtime;

const IMAGE: &str = "ubuntu:latest";

async fn pull() -> ContainerBuilder {
    Container::pull(&IMAGE.parse().unwrap())
        .await
        .expect("container pull")
}

#[test]
fn ubuntu_true() {
    Runtime::new().unwrap().block_on(async {
        let container = pull().await.arg("/bin/true").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}

#[test]
fn ubuntu_ldso() {
    Runtime::new().unwrap().block_on(async {
        let container = pull().await.arg("/usr/lib/x86_64-linux-gnu/ld-2.31.so").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}
