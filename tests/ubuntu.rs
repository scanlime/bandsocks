use bandsocks::{container::ContainerBuilder, Container};
use tokio::runtime::Runtime;

const IMAGE: &str =
    "ubuntu@sha256:a569d854594dae4c70f0efef5f5857eaa3b97cdb1649ce596b113408a0ad5f7f";

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
fn ubuntu_true() {
    env_logger::init();
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("/bin/true").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}

#[test]
fn ubuntu_ldso() {
    Runtime::new().unwrap().block_on(async {
        let container = common()
            .await
            .arg("/usr/lib/x86_64-linux-gnu/ld-2.31.so")
            .spawn()
            .unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}
 */
