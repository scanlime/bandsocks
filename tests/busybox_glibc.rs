use bandsocks::{Container, ContainerBuilder, RuntimeError};
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::io::{BufRead, Cursor};
use tokio::{runtime::Runtime, task};

const IMAGE: &str =
    "busybox:glibc@sha256:052f643f17b56d5b326bd9614698cbeadca9212875090ee089227999ab18c446";

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
fn busybox_sleep_once() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("sleep").arg("0.5").spawn().unwrap();
        let status = container.wait().await.unwrap();
        assert!(status.success());
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
fn busybox_sh_c_echo() {
    Runtime::new().unwrap().block_on(async {
        let output = common()
            .await
            .args(&["sh", "-c", "echo -ne hello; echo ' world'"])
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout_str(), "hello world\n");
    })
}

#[test]
fn busybox_sh_c_loop() {
    Runtime::new().unwrap().block_on(async {
        let output = common()
            .await
            .args(&["sh", "-c", "for i in 0 1 2; do echo -ne $i; done"])
            .output()
            .await
            .unwrap();
        println!("{:?}", output);
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout_str(), "012");
        assert!(output.status.success());
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
            "BusyBox v1.33.0 (2020-12-29 21:52:11 UTC) multi-call binary.\n"
        );
    })
}

#[test]
fn busybox_stat_root() {
    Runtime::new().unwrap().block_on(async {
        let output = common().await.args(&["stat", "/"]).output().await.unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout_str(),
            concat!(
                "  File: /\n",
                "  Size: 0         \tBlocks: 0          IO Block: 512    directory\n",
                "Device: 0h/0d\tInode: 0           Links: 11\n",
                "Access: (0755/drwxr-xr-x)  Uid: (    0/    root)   Gid: (    0/    root)\n",
                "Access: 1970-01-01 00:00:00.000000000 +0000\n",
                "Modify: 1970-01-01 00:00:00.000000000 +0000\n",
                "Change: 1970-01-01 00:00:00.000000000 +0000\n",
            )
        );
    })
}

#[test]
fn busybox_stat_sh() {
    Runtime::new().unwrap().block_on(async {
        let output = common()
            .await
            .args(&["stat", "/bin/sh"])
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout_str(),
            concat!(
                "  File: /bin/sh\n",
                "  Size: 1013200   \tBlocks: 1979       IO Block: 512    regular file\n",
                "Device: 0h/0d\tInode: 2           Links: 400\n",
                "Access: (0755/-rwxr-xr-x)  Uid: (    0/    root)   Gid: (    0/    root)\n",
                "Access: 1970-01-01 00:00:00.000000000 +0000\n",
                "Modify: 2020-12-29 21:53:03.000000000 +0000\n",
                "Change: 1970-01-01 00:00:00.000000000 +0000\n",
            )
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
