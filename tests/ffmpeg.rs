use bandsocks::{Container, ContainerBuilder};
use tokio::runtime::Runtime;

const IMAGE: &str =
    "jrottenberg/ffmpeg:3-scratch@sha256:3396ea2f9b2224de47275cabf8ac85ee765927f6ebdc9d044bb22b7c104fedbd";

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
fn ffmpeg_ldso() {
    Runtime::new().unwrap().block_on(async {
        let output = common()
            .await
            .entrypoint(&["/lib/ld-musl-x86_64.so.1"])
            .output()
            .await
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            output.stderr_str(),
            concat!(
                "musl libc (x86_64)\n",
                "Version 1.1.19\n",
                "Dynamic Program Loader\n",
                "Usage: /lib/ld-musl-x86_64.so.1 [options] [--] pathname [args]\n",
            )
        );
    })
}

/*
#[test]
fn ffmpeg_help() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert_eq!(status.code(), Some(0));
    })
}
*/
