use bandsocks::{container::ContainerBuilder, errors::RuntimeError, Container};
use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::{runtime::Runtime, task};

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

#[test]
fn busybox_sleep_sequential() {
    const NUM: usize = 10;
    Runtime::new().unwrap().block_on(async {
        let mut builder = common().await;
        builder.arg("sleep").arg("0.01");
        for _ in 0..NUM {
            assert!(builder.spawn().unwrap().wait().await.unwrap().success());
        }
    })
}

#[test]
fn busybox_sleep_parallel() {
    const NUM: usize = 10;
    Runtime::new().unwrap().block_on(async {
        let mut builder = common().await;
        let mut tasks = FuturesUnordered::new();
        builder.arg("sleep").arg("2.0");
        for _ in 0..NUM {
            let builder_copy = builder.clone();
            tasks.push(task::spawn(async move {
                Ok::<bool, RuntimeError>(builder_copy.spawn()?.wait().await?.success())
            }));
        }
        let mut results = Vec::new();
        while let Some(result) = tasks.next().await {
            results.push(result.unwrap().unwrap())
        }
        assert_eq!(results, vec![true; NUM])
    })
}
