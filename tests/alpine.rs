use bandsocks::{Container, ContainerBuilder};
use tokio::runtime::Runtime;

const IMAGE: &str =
    "alpine@sha256:d7342993700f8cd7aba8496c2d0e57be0666e80b4c441925fc6f9361fa81d10e";

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
fn alpine_true() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("/bin/true").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}

#[test]
fn alpine_false() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("/bin/false").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(1));
    })
}

#[test]
fn alpine_uname() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("uname").arg("-a").spawn().unwrap();
        let output = container.output().await.unwrap();
        println!("{:?}", output);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout_str(),
            "Linux host 4.0.0-bandsocks #1 SMP x86_64 Linux\n"
        );
    })
}
