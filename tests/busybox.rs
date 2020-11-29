use bandsocks::{container::ContainerBuilder, Container};
use tokio::runtime::Runtime;

const IMAGE: &str =
    "busybox@sha256:e06f93f59fe842fb490ba992bae19fdd5a05373547b52f8184650c2509908114";

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

#[test]
fn busybox_true() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("/bin/true").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}

#[test]
fn busybox_false() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("/bin/false").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(1));
    })
}

#[test]
fn busybox_sleep() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("sleep").arg("0.5").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert!(status.success());
    })
}
