use bandsocks::{Container, ContainerBuilder};
use regex::Regex;
use std::str::from_utf8;
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
 */

#[test]
fn ubuntu_ldso() {
    Runtime::new().unwrap().block_on(async {
        let container = common()
            .await
            .arg("/lib/x86_64-linux-gnu/ld-2.32.so")
            .spawn()
            .unwrap();
        let output = container.output().await.unwrap();
        assert_eq!(output.status.code(), Some(127));
        assert!(output.stdout.is_empty());
        let stderr = from_utf8(&output.stderr).unwrap();
        assert_eq!(
            stderr,
            concat!(
                "Usage: ld.so [OPTION]... EXECUTABLE-FILE [ARGS-FOR-PROGRAM...]\n",
                "You have invoked `ld.so\', the helper program for shared library executables.\n",
                "This program usually lives in the file `/lib/ld.so\', and special directives\n",
                "in executable files using ELF shared libraries tell the system\'s program\n",
                "loader to load the helper program from this file.  This helper program loads\n",
                "the shared libraries needed by the program executable, prepares the program\n",
                "to run, and runs it.  You may invoke this helper program directly from the\n",
                "command line to load and run an ELF executable file; this is like executing\n",
                "that file itself, but always uses this helper program from the file you\n",
                "specified, instead of the helper program file specified in the executable\n",
                "file you run.  This is mostly of use for maintainers to test new versions\n",
                "of this helper program; chances are you did not intend to run this program.\n\n",
                "  --list                list all dependencies and how they are resolved\n",
                "  --verify              verify that given object really is a dynamically linked\n",
                "\t\t\tobject we can handle\n",
                "  --inhibit-cache       Do not use /etc/ld.so.cache\n",
                "  --library-path PATH   use given PATH instead of content of the environment\n",
                "\t\t\tvariable LD_LIBRARY_PATH\n",
                "  --inhibit-rpath LIST  ignore RUNPATH and RPATH information in object names\n",
                "\t\t\tin LIST\n",
                "  --audit LIST          use objects named in LIST as auditors\n",
                "  --preload LIST        preload objects named in LIST\n",
            )
        );
    })
}

#[test]
fn ubuntu_ldso_auxv() {
    Runtime::new().unwrap().block_on(async {
        let container = common()
            .await
            .env("LD_SHOW_AUXV", "1")
            .arg("/lib/x86_64-linux-gnu/ld-2.32.so")
            .spawn()
            .unwrap();
        let output = container.output().await.unwrap();
        assert_eq!(output.status.code(), Some(127));
        let stdout = from_utf8(&output.stdout).unwrap();
        let stderr = from_utf8(&output.stderr).unwrap();
        println!("{:?}", stdout);
        assert!(Regex::new(r"^Usage: ld\.so").unwrap().is_match(&stderr));
        assert!(Regex::new(concat!(
            r"^AT_SYSINFO_EHDR: +0x7ff......000\n",
            r"AT_HWCAP: +........\n",
            r"AT_PAGESZ: +4096\n",
            r"AT_CLKTCK: +100\n",
            r"AT_PHDR: +0x2aa......040\n",
            r"AT_PHENT: +56\n",
            r"AT_PHNUM: +11\n",
            r"AT_BASE: +0x2aa......000\n",
            r"AT_FLAGS: +0x0\n",
            r"AT_ENTRY: +0x2aa......0d0\n",
            r"AT_UID: +0\n",
            r"AT_EUID: +0\n",
            r"AT_GID: +0\n",
            r"AT_EGID: +0\n",
            r"AT_SECURE: +0\n",
            r"AT_RANDOM: +0x7ff......ff0\n",
            r"AT_HWCAP2: +0x0\n",
            r"AT_EXECFN: +/lib/x86_64-linux-gnu/ld-2.32.so\n",
            r"AT_PLATFORM: +x86_64\n$"
        ))
        .unwrap()
        .is_match(&stdout));
    })
}
