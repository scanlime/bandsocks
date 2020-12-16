use bandsocks::{Container, ContainerBuilder};
use std::io::{BufRead, Cursor};
use tokio::runtime::Runtime;

const IMAGE: &str =
    "busybox@sha256:31a54a0cf86d7354788a8265f60ae6acb4b348a67efbcf7c1007dd3cf7af05ab";

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
    });
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
fn busybox_cat_output() {
    Runtime::new().unwrap().block_on(async {
        let output = common()
            .await
            .args(&["cat", "/etc/passwd"])
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout_str(), concat!(
            "root:x:0:0:root:/root:/bin/sh\ndaemon:x:1:1:daemon:/usr/sbin:/bin/false\n",
            "bin:x:2:2:bin:/bin:/bin/false\nsys:x:3:3:sys:/dev:/bin/false\n",
            "sync:x:4:100:sync:/bin:/bin/sync\nmail:x:8:8:mail:/var/spool/mail:/bin/false\n",
            "www-data:x:33:33:www-data:/var/www:/bin/false\noperator:x:37:37:Operator:/var:/bin/false\n",
            "nobody:x:65534:65534:nobody:/home:/bin/false\n"
        ));
    })
}

#[test]
fn busybox_version() {
    Runtime::new().unwrap().block_on(async {
        let output = common()
            .await
            .args(&["busybox", "--help"])
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let mut cursor = Cursor::new(&output.stdout);
        let mut line = String::new();
        cursor.read_line(&mut line).unwrap();
        assert_eq!(
            line,
            "BusyBox v1.32.0 (2020-12-03 00:49:17 UTC) multi-call binary.\n"
        );
    })
}

#[test]
fn busybox_uname() {
    Runtime::new().unwrap().block_on(async {
        let output = common()
            .await
            .args(&["uname", "-a"])
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout_str(),
            "Linux host 4.0.0-bandsocks #1 SMP x86_64 GNU/Linux\n"
        );
    })
}
