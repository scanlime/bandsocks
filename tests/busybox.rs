use bandsocks::{Container, ContainerBuilder, RuntimeError};
use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::{runtime::Runtime, task};

const IMAGE: &str =
    "busybox@sha256:e06f93f59fe842fb490ba992bae19fdd5a05373547b52f8184650c2509908114";

async fn common() -> ContainerBuilder {
    file_limit::set_to_max().unwrap();
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
        log::trace!("done??");
    });
    log::trace!("done?1!");
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
fn busybox_sleep_once() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("sleep").arg("0.5").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert!(status.success());
    })
}

#[test]
fn busybox_sleep_sequential() {
    const NUM: usize = 100;
    Runtime::new().unwrap().block_on(async {
        let builder = common().await.arg("sleep").arg("0.001");
        for _ in 0..NUM {
            assert!(builder
                .clone()
                .spawn()
                .unwrap()
                .wait()
                .await
                .unwrap()
                .success());
        }
    })
}

#[test]
fn busybox_sleep_parallel() {
    const NUM: usize = 100;
    Runtime::new().unwrap().block_on(async {
        let builder = common().await.arg("sleep").arg("5.0");
        let mut tasks = FuturesUnordered::new();
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

#[test]
fn busybox_bool_parallel() {
    const NUM: usize = 100;
    Runtime::new().unwrap().block_on(async {
        let builder = common().await;
        let mut tasks = FuturesUnordered::new();
        for i in 0..NUM {
            let builder = builder.clone().arg(["true", "false"][i & 1]);
            tasks.push(task::spawn(async move {
                Ok::<(usize, Option<i32>), RuntimeError>((i, builder.spawn()?.wait().await?.code()))
            }));
        }
        for _ in 0..NUM {
            let (i, code) = tasks.next().await.unwrap().unwrap().unwrap();
            assert_eq!(code.unwrap(), (i & 1) as i32);
        }
    })
}
