use bandsocks::{Container, ContainerBuilder};
use tokio::runtime::Runtime;

const IMAGE: &str =
    "debian:stable@sha256:12f327b8fe74c597b30a7a2aad24c7711f80b9de3b0fa4d53f20bd00592c7728";

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
fn debian_true() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("true").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}

#[test]
fn debian_false() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("false").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(1));
    })
}

#[test]
fn debian_echo() {
    Runtime::new().unwrap().block_on(async {
        let container = common()
            .await
            .arg("echo")
            .arg("hello")
            .arg("world")
            .spawn()
            .unwrap();
        let output = container.output().await.unwrap();
        println!("{:?}", output);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout_str(), "hello world\n");
    })
}

#[test]
fn super_cow_powers() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("apt").arg("moo").spawn().unwrap();
        let output = container.output().await.unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout_str(),
            concat!(
                "                 (__) \n",
                "                 (oo) \n",
                "           /------\\/ \n",
                "          / |    ||   \n",
                "         *  /\\---/\\ \n",
                "            ~~   ~~   \n",
                "...\"Have you mooed today?\"...\n",
            )
        );
    })
}
