use bandsocks::{Container, ContainerBuilder};
use tokio::runtime::Runtime;

const IMAGE: &str =
    "busybox@sha256:e06f93f59fe842fb490ba992bae19fdd5a05373547b52f8184650c2509908114";

async fn pull() -> ContainerBuilder {
    Container::pull(&IMAGE.parse().unwrap())
        .await
        .expect("container pull")
}

#[test]
fn busybox_true() {
    env_logger::init();
    Runtime::new().unwrap().block_on(async {
        let container = pull().await.arg("/bin/true").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}
