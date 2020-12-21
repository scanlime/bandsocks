use crate::*;

#[test]
fn bools() {
    let mut buf = buffer::IPCBuffer::new();
    buf.push_back(&true).unwrap();
    assert_eq!(buf.as_slice().bytes, &[1]);
    assert_eq!(buf.pop_front::<bool>(), Ok(true));
    assert!(buf.is_empty());
    buf.push_back(&false).unwrap();
    assert_eq!(buf.as_slice().bytes, &[0]);
    assert_eq!(buf.pop_front::<bool>(), Ok(false));
    assert!(buf.is_empty());
    buf.push_back_byte(1).unwrap();
    buf.push_back_byte(0).unwrap();
    assert_eq!(buf.pop_front::<bool>(), Ok(true));
    assert_eq!(buf.pop_front::<bool>(), Ok(false));
    assert!(buf.is_empty());
    buf.push_back_byte(2).unwrap();
    assert_eq!(buf.pop_front::<bool>(), Err(buffer::Error::InvalidValue));
    assert_eq!(buf.as_slice().bytes, &[2]);
}

#[test]
fn options() {
    let mut buf = buffer::IPCBuffer::new();
    buf.push_back(&Some(false)).unwrap();
    buf.push_back(&Some(42u8)).unwrap();
    buf.push_back::<Option<u64>>(&None).unwrap();
    buf.push_back::<Option<()>>(&None).unwrap();
    assert_eq!(buf.as_slice().bytes, &[1, 0, 1, 42, 0, 0]);
    assert_eq!(buf.pop_front::<Option<bool>>(), Ok(Some(false)));
    assert_eq!(buf.pop_front::<Option<u8>>(), Ok(Some(42u8)));
    assert_eq!(buf.pop_front::<Option<u64>>(), Ok(None));
    assert_eq!(buf.pop_front::<Option<()>>(), Ok(None));
    assert!(buf.is_empty());
}

#[test]
fn messages() {
    let msg1 = MessageToSand::Task {
        task: VPid(12345),
        op: ToTask::FileReply(Ok((
            VFile {
                inode: 0x12345678abcdef01,
            },
            SysFd(5),
        ))),
    };
    let msg2 = MessageToSand::Task {
        task: VPid(39503),
        op: ToTask::FileReply(Err(Errno(2333))),
    };
    let msg3 = MessageToSand::Task {
        task: VPid(29862),
        op: ToTask::FileReply(Ok((VFile { inode: 0 }, SysFd(99999)))),
    };
    let msg4 = MessageToSand::Task {
        task: VPid(125),
        op: ToTask::FileReply(Ok((VFile { inode: 777777 }, SysFd(299)))),
    };
    let mut buf = buffer::IPCBuffer::new();
    buf.push_back(&msg1).unwrap();
    buf.push_back(&msg2).unwrap();
    buf.push_back(&msg3).unwrap();
    buf.push_back(&msg4).unwrap();
    assert_eq!(buf.as_slice().bytes.len(), 56);
    assert_eq!(buf.as_slice().files.len(), 3);
    assert_eq!(buf.pop_front::<MessageToSand>(), Ok(msg1));
    assert_eq!(buf.pop_front::<MessageToSand>(), Ok(msg2));
    assert_eq!(buf.pop_front::<MessageToSand>(), Ok(msg3));
    assert_eq!(buf.pop_front::<MessageToSand>(), Ok(msg4));
    assert!(buf.is_empty());
}

#[test]
fn incomplete_message() {
    let mut buf = buffer::IPCBuffer::new();
    assert_eq!(
        buf.pop_front::<MessageToSand>(),
        Err(buffer::Error::UnexpectedEnd)
    );
    buf.extend_bytes(&[0x00]).unwrap();
    assert_eq!(
        buf.pop_front::<MessageToSand>(),
        Err(buffer::Error::UnexpectedEnd)
    );
    buf.push_back_file(SysFd(10)).unwrap();
    buf.extend_bytes(&[0x99]).unwrap();
    assert_eq!(
        buf.pop_front::<MessageToSand>(),
        Err(buffer::Error::UnexpectedEnd)
    );
    buf.push_back_file(SysFd(20)).unwrap();
    assert_eq!(
        buf.pop_front::<MessageToSand>(),
        Err(buffer::Error::UnexpectedEnd)
    );
    buf.extend_bytes(&[0x99, 0x66, 0x66]).unwrap();
    assert_eq!(
        buf.pop_front::<MessageToSand>(),
        Err(buffer::Error::UnexpectedEnd)
    );
    buf.extend_bytes(&[0x00]).unwrap();
    assert_eq!(
        buf.pop_front::<MessageToSand>(),
        Ok(MessageToSand::Task {
            task: VPid(0x66669999),
            op: ToTask::OpenProcessReply(ProcessHandle {
                mem: SysFd(10),
                maps: SysFd(20)
            })
        })
    );
    assert!(buf.is_empty());
}

macro_rules! check {
    ($name:ident, $msg:expr, $t:ty, $bytes:expr, $files:expr) => {
        #[test]
        fn $name() {
            let mut buf = buffer::IPCBuffer::new();
            let msg: $t = $msg;
            let bytes: &[u8] = &$bytes;
            let files: &[SysFd] = &$files;
            buf.push_back(&msg).unwrap();
            assert_eq!(buf.as_slice().bytes, bytes);
            assert_eq!(buf.as_slice().files, files);
            assert_eq!(buf.pop_front::<$t>(), Ok(msg));
            assert!(buf.is_empty());
        }
    };
}

macro_rules! nope {
    ($name: ident, $msg:expr, $t:ty) => {
        #[test]
        fn $name() {
            let mut buf = buffer::IPCBuffer::new();
            let msg: $t = $msg;
            assert_eq!(buf.push_back(&msg), Err(buffer::Error::Unimplemented));
            assert!(buf.is_empty());
        }
    };
}

nope!(no_char, 'n', char);
nope!(no_str, "blah", &str);
nope!(no_f32, 1.0, f32);
nope!(no_f64, 1.0, f64);

check!(u32_1, 0x12345678, u32, [0x78, 0x56, 0x34, 0x12], []);
check!(u32_2, 0x00000000, u32, [0x00, 0x00, 0x00, 0x00], []);
check!(u32_3, 0xffffffff, u32, [0xff, 0xff, 0xff, 0xff], []);
check!(u8_1, 0x42, u8, [0x42], []);
check!(u8_2, 0x00, u8, [0x00], []);
check!(u8_3, 0xff, u8, [0xff], []);
check!(i32_1, 0x7fffffff, i32, [0xff, 0xff, 0xff, 0x7f], []);
check!(i32_2, 0, i32, [0x00, 0x00, 0x00, 0x00], []);
check!(i32_3, -1, i32, [0xff, 0xff, 0xff, 0xff], []);
check!(u16_1, 0xffff, u16, [0xff, 0xff], []);
check!(i16_1, -1, i16, [0xff, 0xff], []);
check!(i8_1, 50, i8, [50], []);
check!(i8_2, 0, i8, [0x00], []);
check!(i8_3, -1, i8, [0xff], []);
check!(u64_1, 0, u64, [0; 8], []);
check!(u64_2, 0xffffffffffffffff, u64, [0xff; 8], []);
check!(i64_1, -1, i64, [0xff; 8], []);
check!(fd_1, SysFd(0x87654321), SysFd, [], [SysFd(0x87654321)]);
check!(fd_2, SysFd(0), SysFd, [], [SysFd(0)]);
check!(fd_ok, Ok(SysFd(123)), Result<SysFd, Errno>, [0], [SysFd(123)]);
check!(fd_err, Err(Errno(-2)), Result<SysFd, Errno>, [1, 0xfe, 0xff, 0xff, 0xff], []);

check!(
    fd_array_1,
    [SysFd(5), SysFd(4), SysFd(3), SysFd(2), SysFd(1)],
    [SysFd; 5],
    [],
    [SysFd(5), SysFd(4), SysFd(3), SysFd(2), SysFd(1)]
);
check!(
    fd_option_array_1,
    [None, Some(SysFd(2)), Some(SysFd(1)), None],
    [Option<SysFd>; 4],
    [0, 1, 1, 0],
    [SysFd(2), SysFd(1)]
);
check!(
    vptr_1,
    VPtr(0x1122334455667788),
    VPtr,
    [0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11],
    []
);
check!(bytes_1, *b"bla", [u8; 3], [98, 108, 97], []);
check!(
    bytes_2,
    (true, *b"blahh", 1),
    (bool, [u8; 5], u32),
    [1, 98, 108, 97, 104, 104, 1, 0, 0, 0],
    []
);
check!(
    tuple_1,
    (true, false, false, 0xabcd, 0xaabbccdd00112233),
    (bool, bool, bool, u16, u64),
    [1, 0, 0, 0xcd, 0xab, 0x33, 0x22, 0x11, 0x00, 0xdd, 0xcc, 0xbb, 0xaa],
    []
);
check!(
    sys_open_1,
    MessageFromSand::Task {
        task: VPid(0x12349955),
        op: FromTask::FileOpen {
            dir: None,
            path: VString(VPtr(0x5544332211009933)),
            mode: 0x55667788,
            flags: 0x34562222
        }
    },
    MessageFromSand,
    [
        0x00, 0x55, 0x99, 0x34, 0x12, 0x02, 0x00, 0x33, 0x99, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55,
        0x22, 0x22, 0x56, 0x34, 0x88, 0x77, 0x66, 0x55,
    ],
    []
);
check!(
    sys_open_2,
    MessageFromSand::Task {
        task: VPid(0x22222222),
        op: FromTask::FileOpen {
            dir: Some(VFile {
                inode: 0x8888777766665555
            },),
            path: VString(VPtr(0x3333333333333333)),
            mode: 0x44444444,
            flags: 0x55555555
        }
    },
    MessageFromSand,
    [
        0x00, 0x22, 0x22, 0x22, 0x22, 0x02, 0x01, 0x55, 0x55, 0x66, 0x66, 0x77, 0x77, 0x88, 0x88,
        0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x55, 0x55, 0x55, 0x55, 0x44, 0x44, 0x44,
        0x44,
    ],
    []
);
check!(
    sys_open_reply_1,
    MessageToSand::Task {
        task: VPid(0x54555657),
        op: ToTask::FileReply(Ok((
            VFile {
                inode: 0x3333444455556666
            },
            SysFd(42)
        ))),
    },
    MessageToSand,
    [0x00, 0x57, 0x56, 0x55, 0x54, 0x01, 0x00, 0x66, 0x66, 0x55, 0x55, 0x44, 0x44, 0x33, 0x33],
    [SysFd(42)]
);
check!(
    sys_open_reply_2,
    MessageToSand::Task {
        task: VPid(0x11223344),
        op: ToTask::FileReply(Err(Errno(-10)))
    },
    MessageToSand,
    [0x00, 0x44, 0x33, 0x22, 0x11, 0x01, 0x01, 0xf6, 0xff, 0xff, 0xff],
    []
);
check!(
    process_open_reply_1,
    MessageToSand::Task {
        task: VPid(0x66669999),
        op: ToTask::OpenProcessReply(ProcessHandle {
            mem: SysFd(10),
            maps: SysFd(20)
        })
    },
    MessageToSand,
    [0x00, 0x99, 0x99, 0x66, 0x66, 0],
    [SysFd(10), SysFd(20)]
);
