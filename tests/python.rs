use bandsocks::{Container, ContainerBuilder};
use tokio::runtime::Runtime;

const IMAGE: &str =
    "python:rc-buster@sha256:d2ef9bb582580d98351897b8812298ea25d8140614075e64cccaff21bdeb04d6";

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
fn python_os_file() {
    Runtime::new().unwrap().block_on(async {
        let container = common()
            .await
            .arg("python")
            .arg("-c")
            .arg(
                r"
import os
print(os.__file__)
",
            )
            .spawn()
            .unwrap();
        let output = container.output().await.unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout_str(), "/usr/local/lib/python3.10/os.py\n");
    })
}
