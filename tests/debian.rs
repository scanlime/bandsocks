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
fn debian_uname() {
    Runtime::new().unwrap().block_on(async {
        let container = common().await.arg("uname").arg("-a").spawn().unwrap();
        let output = container.output().await.unwrap();
        println!("{:?}", output);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout_str(),
            "Linux host 4.0.0-bandsocks #1 SMP x86_64 GNU/Linux\n"
        );
    })
}

#[test]
fn debian_readlink_sh() {
    Runtime::new().unwrap().block_on(async {
        let container = common()
            .await
            .arg("readlink")
            .arg("/bin/sh")
            .spawn()
            .unwrap();
        let output = container.output().await.unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout_str(), "dash\n");
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

#[test]
fn debian_ls_l_etc() {
    Runtime::new().unwrap().block_on(async {
        let container = common()
            .await
            .arg("ls")
            .arg("-l")
            .arg("/etc")
            .spawn()
            .unwrap();
        let output = container.output().await.unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout_str(),
            concat!(
                "total 50\n",
                "-rw-r--r-- 1 root root    2981 Dec  9 23:22 adduser.conf\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 alternatives\n",
                "drwxr-xr-x 5 root root       0 Dec  9 23:22 apt\n",
                "-rw-r--r-- 1 root root    1994 Apr 18  2019 bash.bashrc\n",
                "-rw-r--r-- 1 root root     367 Mar  2  2018 bindresvport.blacklist\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 cron.daily\n",
                "-rw-r--r-- 1 root root    2969 Feb 26  2019 debconf.conf\n",
                "-rw-r--r-- 1 root root       5 Nov 22 12:37 debian_version\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 default\n",
                "-rw-r--r-- 1 root root     604 Jun 26  2016 deluser.conf\n",
                "drwxr-xr-x 2 root root       0 Dec  9 23:22 dpkg\n",
                "-rw-r--r-- 1 root root       0 Dec  9 23:22 environment\n",
                "-rw-r--r-- 1 root root      37 Dec  9 23:22 fstab\n",
                "-rw-r--r-- 1 root root    2584 Aug  1  2018 gai.conf\n",
                "-rw-r--r-- 1 root root     446 Dec  9 23:22 group\n",
                "-rw-r--r-- 1 root root     446 Dec  9 23:22 group-\n",
                "-rw-r----- 1 root shadow   374 Dec  9 23:22 gshadow\n",
                "-rw-r--r-- 1 root root       9 Aug  7  2006 host.conf\n",
                "-rw-r--r-- 1 root root      14 Dec  9 23:22 hostname\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 init.d\n",
                "drwxr-xr-x 2 root root       0 Dec  9 23:22 iproute2\n",
                "-rw-r--r-- 1 root root      27 Nov 22 12:37 issue\n",
                "-rw-r--r-- 1 root root      20 Nov 22 12:37 issue.net\n",
                "drwxr-xr-x 1 root root       0 May 12  2020 kernel\n",
                "-rw-r--r-- 1 root root    7104 Dec  9 23:22 ld.so.cache\n",
                "-rw-r--r-- 1 root root      34 Mar  2  2018 ld.so.conf\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 ld.so.conf.d\n",
                "-rw-r--r-- 1 root root     191 Apr 25  2019 libaudit.conf\n",
                "lrwxrwxrwx 1 root root       0 Dec  9 23:22 localtime -> /usr/share/zoneinfo/Etc/UTC\n",
                "-rw-r--r-- 1 root root   10477 Jul 27  2018 login.defs\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 logrotate.d\n",
                "-rw-r--r-- 1 root root      33 Dec  9 23:22 machine-id\n",
                "-rw-r--r-- 1 root root     812 Jan 10  2020 mke2fs.conf\n",
                "-rw-r--r-- 1 root root     286 Nov 22 12:37 motd\n",
                "-rw-r--r-- 1 root root     494 Feb 10  2019 nsswitch.conf\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 opt\n",
                "lrwxrwxrwx 1 root root       0 Nov 22 12:37 os-release -> ../usr/lib/os-release\n",
                "-rw-r--r-- 1 root root     552 Feb 14  2019 pam.conf\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 pam.d\n",
                "-rw-r--r-- 1 root root     926 Dec  9 23:22 passwd\n",
                "-rw-r--r-- 1 root root     926 Dec  9 23:22 passwd-\n",
                "-rw-r--r-- 1 root root     767 Mar  4  2016 profile\n",
                "drwxr-xr-x 0 root root       0 Nov 22 12:37 profile.d\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 rc0.d\n",
                "drwxr-xr-x 0 root root       0 Dec  3  2018 rc1.d\n",
                "drwxr-xr-x 0 root root       0 Dec  3  2018 rc2.d\n",
                "drwxr-xr-x 0 root root       0 Dec  3  2018 rc3.d\n",
                "drwxr-xr-x 0 root root       0 Dec  3  2018 rc4.d\n",
                "drwxr-xr-x 0 root root       0 Dec  3  2018 rc5.d\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 rc6.d\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 rcS.d\n",
                "-rw-r--r-- 1 root root     104 Dec  9 23:22 resolv.conf\n",
                "lrwxrwxrwx 1 root root       0 Apr 23  2019 rmt -> /usr/sbin/rmt\n",
                "-rw-r--r-- 1 root root    4141 Jul 27  2018 securetty\n",
                "drwxr-xr-x 2 root root       0 Dec  9 23:22 security\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 selinux\n",
                "-rw-r----- 1 root shadow   501 Dec  9 23:22 shadow\n",
                "-rw-r----- 1 root shadow   501 Dec  9 23:22 shadow-\n",
                "-rw-r--r-- 1 root root      73 Dec  9 23:22 shells\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 skel\n",
                "-rw-r--r-- 1 root root       0 Dec  9 23:22 subgid\n",
                "-rw-r--r-- 1 root root       0 Dec  9 23:22 subuid\n",
                "drwxr-xr-x 1 root root       0 Dec  3  2018 systemd\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 terminfo\n",
                "-rw-r--r-- 1 root root       8 Dec  9 23:22 timezone\n",
                "drwxr-xr-x 0 root root       0 Dec  9 23:22 update-motd.d\n",
                "-rw-r--r-- 1 root root     642 Mar  1  2019 xattr.conf\n",
            )
        );
    })
}

#[test]
fn debian_cat_etc_shadow() {
    Runtime::new().unwrap().block_on(async {
        let container = common()
            .await
            .arg("cat")
            .arg("/etc/shadow")
            .spawn()
            .unwrap();
        let output = container.output().await.unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout_str(),
            concat!(
                "root:*:18605:0:99999:7:::\n",
                "daemon:*:18605:0:99999:7:::\n",
                "bin:*:18605:0:99999:7:::\n",
                "sys:*:18605:0:99999:7:::\n",
                "sync:*:18605:0:99999:7:::\n",
                "games:*:18605:0:99999:7:::\n",
                "man:*:18605:0:99999:7:::\n",
                "lp:*:18605:0:99999:7:::\n",
                "mail:*:18605:0:99999:7:::\n",
                "news:*:18605:0:99999:7:::\n",
                "uucp:*:18605:0:99999:7:::\n",
                "proxy:*:18605:0:99999:7:::\n",
                "www-data:*:18605:0:99999:7:::\n",
                "backup:*:18605:0:99999:7:::\n",
                "list:*:18605:0:99999:7:::\n",
                "irc:*:18605:0:99999:7:::\n",
                "gnats:*:18605:0:99999:7:::\n",
                "nobody:*:18605:0:99999:7:::\n",
                "_apt:*:18605:0:99999:7:::\n",
            )
        );
    })
}
