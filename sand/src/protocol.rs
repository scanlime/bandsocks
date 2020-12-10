// The protocol is defined here canonically and then imported
// by the runtime crate along with our finished binary.
// This depends on only: core, serde, generic-array

/// Exit codes returned by the sand process
#[allow(dead_code)]
pub mod exit {
    pub const EXIT_OK: usize = 0;
    pub const EXIT_PANIC: usize = 60;
    pub const EXIT_DISCONNECTED: usize = 61;
    pub const EXIT_IO_ERROR: usize = 62;
}

/// Any message sent from the IPC server to the sand process
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum MessageToSand {
    Task {
        task: VPid,
        op: ToTask,
    },
    Init {
        args: SysFd,
        tracer_settings: TracerSettings,
    },
}

/// Any message sent from the sand process to the IPC server
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum MessageFromSand {
    Task { task: VPid, op: FromTask },
}

/// Fixed size header for the variable sized initial args data
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct InitArgsHeader {
    pub dir_len: usize,
    pub filename_len: usize,
    pub argv_len: usize,
    pub arg_count: usize,
    pub envp_len: usize,
    pub env_count: usize,
}

impl InitArgsHeader {
    #[allow(dead_code)]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self as *const InitArgsHeader as *const u8,
                core::mem::size_of_val(self),
            )
        }
    }

    #[allow(dead_code)]
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self as *mut InitArgsHeader as *mut u8,
                core::mem::size_of_val(self),
            )
        }
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Deserialize, Serialize)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum LogMessage {
    Emulated(abi::Syscall),
    Remote(abi::Syscall),
    Signal(u8, abi::UserRegs),
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct TracerSettings {
    pub max_log_level: LogLevel,
    pub instruction_trace: bool,
}

/// A message delivered to one of the lightweight tasks in the tracer
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum ToTask {
    OpenProcessReply(ProcessHandle),
    FileReply(Result<SysFd, Errno>),
    FileStatReply(Result<FileStat, Errno>),
    SizeReply(Result<usize, Errno>),
    Reply(Result<(), Errno>),
}

/// A message originating from one lightweight task in the tracer
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum FromTask {
    OpenProcess(SysPid),
    FileAccess {
        dir: Option<SysFd>,
        path: VString,
        mode: i32,
    },
    FileOpen {
        dir: Option<SysFd>,
        path: VString,
        flags: i32,
        mode: i32,
    },
    FileStat {
        fd: Option<SysFd>,
        path: Option<VString>,
        nofollow: bool,
    },
    ProcessKill(VPid, Signal),
    ChangeWorkingDir(VString),
    GetWorkingDir(VString, usize),
    Exited(i32),
    Log(LogLevel, LogMessage),
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct FileStat {
    // to do
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProcessHandle {
    pub mem: SysFd,
    pub maps: SysFd,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Hash32)]
#[repr(C)]
pub struct SysFd(pub u32);

impl core::default::Default for SysFd {
    fn default() -> Self {
        SysFd(!0u32)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Hash32, Serialize, Deserialize)]
#[repr(C)]
pub struct SysPid(pub u32);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct Signal(pub u32);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct Errno(pub i32);

#[derive(
    Debug, PartialEq, Eq, Ord, PartialOrd, Copy, Clone, Hash, Hash32, Serialize, Deserialize,
)]
#[repr(C)]
pub struct VPid(pub u32);

#[derive(PartialEq, Eq, Ord, PartialOrd, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct VPtr(pub usize);

impl core::fmt::Debug for VPtr {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "VPtr({:x?})", self.0)
    }
}

impl VPtr {
    #[allow(dead_code)]
    pub fn null() -> VPtr {
        VPtr(0)
    }

    #[allow(dead_code)]
    pub fn add(&self, count: usize) -> VPtr {
        VPtr(self.0 + count)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct VString(pub VPtr);

/// Definitions that overlap between the kernel ABI and the sand IPC protocol
#[allow(dead_code)]
pub mod abi {

    #[derive(PartialEq, Eq, Ord, PartialOrd, Clone, Serialize, Deserialize)]
    #[repr(C)]
    pub struct Syscall {
        pub nr: isize,
        pub args: [isize; 6],
        pub ret: isize,
        pub ip: usize,
        pub sp: usize,
    }

    impl core::fmt::Debug for Syscall {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            write!(
                f,
                "SYS_{:?} {:x?} -> {:?} (ip={:x?} sp={:x?})",
                self.nr, self.args, self.ret, self.ip, self.sp
            )
        }
    }

    impl Syscall {
        pub fn from_regs(regs: &UserRegs) -> Self {
            Syscall {
                ip: regs.ip,
                sp: regs.sp,
                nr: regs.orig_ax as isize,
                ret: regs.ax as isize,
                args: [
                    regs.di as isize,
                    regs.si as isize,
                    regs.dx as isize,
                    regs.r10 as isize,
                    regs.r8 as isize,
                    regs.r9 as isize,
                ],
            }
        }

        pub fn args_to_regs(args: &[isize], regs: &mut UserRegs) {
            assert!(args.len() <= 6);
            regs.di = *args.get(0).unwrap_or(&0) as usize;
            regs.si = *args.get(1).unwrap_or(&0) as usize;
            regs.dx = *args.get(2).unwrap_or(&0) as usize;
            regs.r10 = *args.get(3).unwrap_or(&0) as usize;
            regs.r8 = *args.get(4).unwrap_or(&0) as usize;
            regs.r9 = *args.get(5).unwrap_or(&0) as usize;
        }

        pub fn nr_to_regs(nr: isize, regs: &mut UserRegs) {
            regs.ax = nr as usize;
        }

        pub fn ret_to_regs(ret_data: isize, regs: &mut UserRegs) {
            regs.ax = ret_data as usize;
        }

        pub fn ret_from_regs(regs: &UserRegs) -> isize {
            regs.ax as isize
        }

        // syscall number to resume, or SYSCALL_BLOCKED to skip
        pub fn orig_nr_to_regs(nr: isize, regs: &mut UserRegs) {
            regs.orig_ax = nr as usize;
        }
    }

    // user_regs_struct
    // linux/arch/x86/include/asm/user_64.h
    // linux/include/asm/user_64.h
    #[derive(Default, PartialEq, Eq, Ord, PartialOrd, Clone, Serialize, Deserialize)]
    #[repr(C)]
    pub struct UserRegs {
        pub r15: usize,
        pub r14: usize,
        pub r13: usize,
        pub r12: usize,
        pub bp: usize,
        pub bx: usize,
        pub r11: usize,
        pub r10: usize,
        pub r9: usize,
        pub r8: usize,
        pub ax: usize,
        pub cx: usize,
        pub dx: usize,
        pub si: usize,
        pub di: usize,
        pub orig_ax: usize,
        pub ip: usize,
        pub cs: usize,
        pub flags: usize,
        pub sp: usize,
        pub ss: usize,
        pub fs_base: usize,
        pub gs_base: usize,
        pub ds: usize,
        pub es: usize,
        pub fs: usize,
        pub gs: usize,
    }

    impl core::fmt::Debug for UserRegs {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            write!(
                f,
                concat!(
                "UserRegs {{\n",
                "  cs={:16x}  ip={:16x}  ss={:16x}  sp={:16x}  bp={:16x} oax={:16x}\n",
                "  ax={:16x}  di={:16x}  si={:16x}  dx={:16x} r10={:16x}  r8={:16x}  r9={:16x}\n",
                "  bx={:16x}  cx={:16x} r11={:16x} r12={:16x} r13={:16x} r14={:16x} r15={:16x}\n",
                "  ds={:16x}  es={:16x}  fs={:16x}  gs={:16x} fs@={:16x} gs@={:16x} flg={:16x}\n",
                "}}"
            ),
                self.cs,
                self.ip,
                self.ss,
                self.sp,
                self.bp,
                self.orig_ax,
                self.ax,
                self.di,
                self.si,
                self.dx,
                self.r10,
                self.r8,
                self.r9,
                self.bx,
                self.cx,
                self.r11,
                self.r12,
                self.r13,
                self.r14,
                self.r15,
                self.ds,
                self.es,
                self.fs,
                self.gs,
                self.fs_base,
                self.gs_base,
                self.flags,
            )
        }
    }
}

pub mod buffer {
    use super::{de, ser, SysFd};
    use core::{fmt, ops::Range};
    use generic_array::{typenum::*, ArrayLength, GenericArray};
    use serde::{de::DeserializeOwned, Serialize};

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum Error {
        Unimplemented,
        UnexpectedEnd,
        BufferFull,
        InvalidValue,
        Serialize,
        Deserialize,
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{:?}", self)
        }
    }

    pub type Result<T> = core::result::Result<T, Error>;
    pub type BytesMax = U4096;
    pub type FilesMax = U128;

    #[derive(Default)]
    pub struct IPCBuffer {
        bytes: Queue<u8, BytesMax>,
        files: Queue<SysFd, FilesMax>,
    }

    #[derive(Default)]
    struct Queue<T: Clone, N: ArrayLength<T>> {
        array: GenericArray<T, N>,
        range: Range<usize>,
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct IPCSlice<'a> {
        pub bytes: &'a [u8],
        pub files: &'a [SysFd],
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct IPCSliceMut<'a> {
        pub bytes: &'a mut [u8],
        pub files: &'a mut [SysFd],
    }

    impl<T: Copy, N: ArrayLength<T>> Queue<T, N> {
        fn is_empty(&self) -> bool {
            self.range.is_empty()
        }

        fn push_back(&mut self, item: T) -> Result<()> {
            if self.range.end < self.array.len() {
                self.array[self.range.end] = item;
                self.range.end += 1;
                Ok(())
            } else {
                Err(Error::BufferFull)
            }
        }

        fn extend(&mut self, items: &[T]) -> Result<()> {
            let new_end = self.range.end + items.len();
            if new_end > self.array.len() {
                Err(Error::BufferFull)
            } else {
                self.array[self.range.end..new_end].clone_from_slice(items);
                self.range.end = new_end;
                Ok(())
            }
        }

        fn pop_front(&mut self, count: usize) {
            self.range.start += count;
            assert!(self.range.start <= self.range.end);
        }

        fn as_slice(&self) -> &[T] {
            &self.array[self.range.clone()]
        }

        fn begin_fill(&mut self) -> &mut [T] {
            let prev_partial_range = self.range.clone();
            let new_partial_range = 0..prev_partial_range.end - prev_partial_range.start;
            let new_empty_range = new_partial_range.end..self.array.len();
            self.array.copy_within(prev_partial_range, 0);
            self.range = new_partial_range;
            &mut self.array[new_empty_range]
        }

        fn commit_fill(&mut self, len: usize) {
            let new_end = self.range.end + len;
            assert!(new_end <= self.array.len());
            self.range.end = new_end;
        }

        fn front(&self, len: usize) -> Result<&[T]> {
            let slice = self.as_slice();
            if len <= slice.len() {
                Ok(&slice[..len])
            } else {
                Err(Error::UnexpectedEnd)
            }
        }
    }

    impl<'a> IPCBuffer {
        pub fn new() -> Self {
            Default::default()
        }

        pub fn as_slice(&'a self) -> IPCSlice<'a> {
            IPCSlice {
                bytes: self.bytes.as_slice(),
                files: self.files.as_slice(),
            }
        }

        pub fn begin_fill(&'a mut self) -> IPCSliceMut<'a> {
            IPCSliceMut {
                bytes: self.bytes.begin_fill(),
                files: self.files.begin_fill(),
            }
        }

        pub fn commit_fill(&'a mut self, num_bytes: usize, num_files: usize) {
            self.bytes.commit_fill(num_bytes);
            self.files.commit_fill(num_files);
        }

        pub fn is_empty(&self) -> bool {
            self.bytes.is_empty() && self.files.is_empty()
        }

        pub fn push_back<T: Serialize>(&mut self, message: &T) -> Result<()> {
            let mut serializer = ser::IPCSerializer::new(self);
            message.serialize(&mut serializer)
        }

        pub fn pop_front<T: Clone + DeserializeOwned>(&'a mut self) -> Result<T> {
            let saved_bytes_range = self.bytes.range.clone();
            let saved_files_range = self.files.range.clone();
            let mut deserializer = de::IPCDeserializer::new(self);
            let result = T::deserialize(&mut deserializer);
            if result.is_err() {
                // Rewind the pop on error, to recover after a partial read
                self.bytes.range = saved_bytes_range;
                self.files.range = saved_files_range;
            }
            result
        }

        pub fn extend_bytes(&mut self, data: &[u8]) -> Result<()> {
            self.bytes.extend(data)
        }

        pub fn push_back_byte(&mut self, data: u8) -> Result<()> {
            self.bytes.push_back(data)
        }

        pub fn push_back_file(&mut self, file: SysFd) -> Result<()> {
            self.files.push_back(file)
        }

        pub fn front_bytes(&self, len: usize) -> Result<&[u8]> {
            self.bytes.front(len)
        }

        pub fn front_files(&self, len: usize) -> Result<&[SysFd]> {
            self.files.front(len)
        }

        pub fn pop_front_bytes(&mut self, len: usize) {
            self.bytes.pop_front(len)
        }

        pub fn pop_front_files(&mut self, len: usize) {
            self.files.pop_front(len)
        }

        pub fn pop_front_byte(&mut self) -> Result<u8> {
            let result = self.front_bytes(1)?[0];
            self.pop_front_bytes(1);
            Ok(result)
        }

        pub fn pop_front_file(&mut self) -> Result<SysFd> {
            let result = self.front_files(1)?[0];
            self.pop_front_files(1);
            Ok(result)
        }
    }
}

mod ser {
    use super::{
        buffer::{Error, IPCBuffer, Result},
        SysFd,
    };
    use core::{fmt::Display, result};
    use serde::{ser, ser::SerializeTupleStruct};

    const SYSFD: &str = "SysFd@ser";

    pub struct IPCSerializer<'a> {
        output: &'a mut IPCBuffer,
        in_sysfd: bool,
    }

    impl<'a> IPCSerializer<'a> {
        pub fn new(output: &'a mut IPCBuffer) -> Self {
            IPCSerializer {
                output,
                in_sysfd: false,
            }
        }
    }

    impl ser::Serialize for SysFd {
        fn serialize<S: ser::Serializer>(&self, serializer: S) -> result::Result<S::Ok, S::Error> {
            let mut tuple = serializer.serialize_tuple_struct(SYSFD, 1)?;
            tuple.serialize_field(&self.0)?;
            tuple.end()
        }
    }

    impl ser::StdError for Error {}

    impl ser::Error for Error {
        fn custom<T: Display>(_msg: T) -> Self {
            Error::Serialize
        }
    }

    macro_rules! to_le_bytes {
        ($gen_fn:ident, $num:ty ) => {
            fn $gen_fn(self, v: $num) -> Result<()> {
                assert_eq!(self.in_sysfd, false);
                self.output.extend_bytes(&v.to_le_bytes())
            }
        };
    }

    impl<'a, 'b> ser::Serializer for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;
        type SerializeSeq = Self;
        type SerializeTuple = Self;
        type SerializeTupleStruct = Self;
        type SerializeTupleVariant = Self;
        type SerializeMap = Self;
        type SerializeStruct = Self;
        type SerializeStructVariant = Self;

        fn is_human_readable(&self) -> bool {
            false
        }

        fn collect_str<T: ?Sized + Display>(self, _v: &T) -> Result<()> {
            Err(Error::Unimplemented)
        }

        fn serialize_bool(self, v: bool) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            self.output.push_back_byte(v as u8)
        }

        fn serialize_f32(self, _v: f32) -> Result<()> {
            Err(Error::Unimplemented)
        }

        fn serialize_f64(self, _v: f64) -> Result<()> {
            Err(Error::Unimplemented)
        }

        to_le_bytes!(serialize_u16, u16);
        to_le_bytes!(serialize_i16, i16);
        to_le_bytes!(serialize_i32, i32);
        to_le_bytes!(serialize_u64, u64);
        to_le_bytes!(serialize_i64, i64);

        fn serialize_u32(self, v: u32) -> Result<()> {
            if self.in_sysfd {
                self.output.push_back_file(SysFd(v))
            } else {
                self.output.extend_bytes(&v.to_le_bytes())
            }
        }

        fn serialize_none(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            self.output.push_back_byte(0)
        }

        fn serialize_some<T: ?Sized + ser::Serialize>(self, v: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            self.output.push_back_byte(1)?;
            v.serialize(self)
        }

        fn serialize_i8(self, v: i8) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            self.output.push_back_byte(v as u8)
        }

        fn serialize_u8(self, v: u8) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            self.output.push_back_byte(v)
        }

        fn serialize_unit(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }

        fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
            self.serialize_unit()
        }

        fn serialize_unit_variant(
            self,
            _name: &'static str,
            variant_index: u32,
            _var: &'static str,
        ) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            if variant_index < 0x100 {
                self.output.push_back_byte(variant_index as u8)
            } else {
                Err(Error::InvalidValue)
            }
        }

        fn serialize_char(self, _v: char) -> Result<()> {
            Err(Error::Unimplemented)
        }

        fn serialize_str(self, _v: &str) -> Result<()> {
            Err(Error::Unimplemented)
        }

        fn serialize_bytes(self, _v: &[u8]) -> Result<()> {
            Err(Error::Unimplemented)
        }

        fn serialize_newtype_struct<T>(self, _: &'static str, value: &T) -> Result<()>
        where
            T: ?Sized + ser::Serialize,
        {
            assert_eq!(self.in_sysfd, false);
            value.serialize(self)
        }

        fn serialize_newtype_variant<T>(
            self,
            name: &'static str,
            variant_index: u32,
            variant: &'static str,
            value: &T,
        ) -> Result<()>
        where
            T: ?Sized + ser::Serialize,
        {
            assert_eq!(self.in_sysfd, false);
            self.serialize_unit_variant(name, variant_index, variant)?;
            value.serialize(self)
        }

        fn serialize_tuple_struct(self, name: &'static str, _len: usize) -> Result<Self> {
            assert_eq!(self.in_sysfd, false);
            self.in_sysfd = name == SYSFD;
            Ok(self)
        }

        fn serialize_seq(self, _len: Option<usize>) -> Result<Self> {
            assert_eq!(self.in_sysfd, false);
            Ok(self)
        }

        fn serialize_tuple(self, _len: usize) -> Result<Self> {
            assert_eq!(self.in_sysfd, false);
            Ok(self)
        }

        fn serialize_map(self, _len: Option<usize>) -> Result<Self> {
            assert_eq!(self.in_sysfd, false);
            Ok(self)
        }

        fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self> {
            assert_eq!(self.in_sysfd, false);
            Ok(self)
        }

        fn serialize_tuple_variant(
            self,
            name: &'static str,
            variant_index: u32,
            variant: &'static str,
            _len: usize,
        ) -> Result<Self> {
            self.serialize_unit_variant(name, variant_index, variant)?;
            Ok(self)
        }

        fn serialize_struct_variant(
            self,
            name: &'static str,
            variant_index: u32,
            variant: &'static str,
            _len: usize,
        ) -> Result<Self> {
            self.serialize_unit_variant(name, variant_index, variant)?;
            Ok(self)
        }
    }

    impl<'a, 'b> ser::SerializeSeq for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_element<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeTuple for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_element<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeTupleStruct for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_field<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            self.in_sysfd = false;
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeTupleVariant for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_field<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeMap for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_key<T: ?Sized + ser::Serialize>(&mut self, key: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            key.serialize(&mut **self)
        }

        fn serialize_value<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeStruct for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_field<T>(&mut self, _name: &'static str, value: &T) -> Result<()>
        where
            T: ?Sized + ser::Serialize,
        {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }

    impl<'a, 'b> ser::SerializeStructVariant for &'b mut IPCSerializer<'a> {
        type Ok = ();
        type Error = Error;

        fn serialize_field<T>(&mut self, _name: &'static str, value: &T) -> Result<()>
        where
            T: ?Sized + ser::Serialize,
        {
            assert_eq!(self.in_sysfd, false);
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<()> {
            assert_eq!(self.in_sysfd, false);
            Ok(())
        }
    }
}

mod de {
    use super::{
        buffer::{Error, IPCBuffer, Result},
        SysFd,
    };
    use core::{fmt, fmt::Display, result};
    use serde::{de, de::IntoDeserializer};

    const SYSFD: &str = "SysFd@de";

    pub struct IPCDeserializer<'d> {
        input: &'d mut IPCBuffer,
    }

    impl<'a> IPCDeserializer<'a> {
        pub fn new(input: &'a mut IPCBuffer) -> Self {
            IPCDeserializer { input }
        }
    }

    impl<'d> de::Deserialize<'d> for SysFd {
        fn deserialize<D: de::Deserializer<'d>>(deserializer: D) -> result::Result<Self, D::Error> {
            struct SysFdVisitor;
            impl<'d> de::Visitor<'d> for SysFdVisitor {
                type Value = SysFd;

                fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                    formatter.write_str("struct SysFD")
                }

                fn visit_u32<E>(self, v: u32) -> result::Result<SysFd, E> {
                    Ok(SysFd(v))
                }
            }
            deserializer.deserialize_tuple_struct(SYSFD, 1, SysFdVisitor)
        }
    }

    impl de::Error for Error {
        fn custom<T: Display>(_msg: T) -> Self {
            Error::Deserialize
        }
    }

    macro_rules! from_le_bytes {
        ($gen_fn:ident, $visit_fn:ident, $num:ty, $len:expr) => {
            fn $gen_fn<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
                let mut bytes = [0u8; $len];
                bytes[..].copy_from_slice(self.input.front_bytes($len)?);
                self.input.pop_front_bytes($len);
                visitor.$visit_fn(<$num>::from_le_bytes(bytes))
            }
        };
    }

    impl<'d> IPCDeserializer<'d> {
        fn deserialize_sysfd<'a, V: de::Visitor<'d>>(&'a mut self, visitor: V) -> Result<V::Value> {
            let file = self.input.pop_front_file()?;
            visitor.visit_u32(file.0)
        }
    }

    impl<'d, 'a> de::Deserializer<'d> for &'a mut IPCDeserializer<'d> {
        type Error = Error;

        fn is_human_readable(&self) -> bool {
            false
        }

        fn deserialize_any<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_byte_buf<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_bytes<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_char<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_f32<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_f64<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_identifier<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_ignored_any<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_str<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_string<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        from_le_bytes!(deserialize_u16, visit_u16, u16, 2);
        from_le_bytes!(deserialize_i16, visit_i16, i16, 2);
        from_le_bytes!(deserialize_u32, visit_u32, u32, 4);
        from_le_bytes!(deserialize_i32, visit_i32, i32, 4);
        from_le_bytes!(deserialize_u64, visit_u64, u64, 8);
        from_le_bytes!(deserialize_i64, visit_i64, i64, 8);

        fn deserialize_u8<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
            visitor.visit_u8(self.input.pop_front_byte()?)
        }

        fn deserialize_i8<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
            visitor.visit_i8(self.input.pop_front_byte()? as i8)
        }

        fn deserialize_bool<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
            match self.input.pop_front_byte()? {
                0 => visitor.visit_bool(false),
                1 => visitor.visit_bool(true),
                _ => Err(Error::InvalidValue),
            }
        }

        fn deserialize_option<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
            match self.input.pop_front_byte()? {
                0 => visitor.visit_none(),
                1 => visitor.visit_some(self),
                _ => Err(Error::InvalidValue),
            }
        }

        fn deserialize_unit<V: de::Visitor<'d>>(self, visitor: V) -> Result<V::Value> {
            visitor.visit_unit()
        }

        fn deserialize_unit_struct<V: de::Visitor<'d>>(
            self,
            _name: &'static str,
            visitor: V,
        ) -> Result<V::Value> {
            visitor.visit_unit()
        }

        fn deserialize_map<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_seq<V: de::Visitor<'d>>(self, _visitor: V) -> Result<V::Value> {
            Err(Error::Unimplemented)
        }

        fn deserialize_tuple<V: de::Visitor<'d>>(self, len: usize, visitor: V) -> Result<V::Value> {
            struct SeqAccess<'d, 'a> {
                deserializer: &'a mut IPCDeserializer<'d>,
                len: usize,
            }

            impl<'d, 'a> de::SeqAccess<'d> for SeqAccess<'d, 'a> {
                type Error = Error;

                fn size_hint(&self) -> Option<usize> {
                    Some(self.len)
                }

                fn next_element_seed<S>(&mut self, seed: S) -> Result<Option<S::Value>>
                where
                    S: de::DeserializeSeed<'d>,
                {
                    if self.len > 0 {
                        self.len -= 1;
                        Ok(Some(de::DeserializeSeed::deserialize(
                            seed,
                            &mut *self.deserializer,
                        )?))
                    } else {
                        Ok(None)
                    }
                }
            }

            visitor.visit_seq(SeqAccess {
                deserializer: self,
                len,
            })
        }

        fn deserialize_tuple_struct<V: de::Visitor<'d>>(
            self,
            name: &'static str,
            len: usize,
            visitor: V,
        ) -> Result<V::Value> {
            if name == SYSFD {
                assert_eq!(len, 1);
                self.deserialize_sysfd(visitor)
            } else {
                self.deserialize_tuple(len, visitor)
            }
        }

        fn deserialize_enum<V: de::Visitor<'d>>(
            self,
            _name: &'static str,
            _variants: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value> {
            visitor.visit_enum(self)
        }

        fn deserialize_newtype_struct<V: de::Visitor<'d>>(
            self,
            _name: &'static str,
            visitor: V,
        ) -> Result<V::Value> {
            visitor.visit_newtype_struct(self)
        }

        fn deserialize_struct<V: de::Visitor<'d>>(
            self,
            _name: &'static str,
            fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value> {
            self.deserialize_tuple(fields.len(), visitor)
        }
    }

    impl<'d, 'a> de::VariantAccess<'d> for &'a mut IPCDeserializer<'d> {
        type Error = Error;

        fn unit_variant(self) -> Result<()> {
            Ok(())
        }

        fn newtype_variant_seed<V: de::DeserializeSeed<'d>>(self, seed: V) -> Result<V::Value> {
            de::DeserializeSeed::deserialize(seed, self)
        }

        fn tuple_variant<V: de::Visitor<'d>>(self, len: usize, visitor: V) -> Result<V::Value> {
            de::Deserializer::deserialize_tuple(self, len, visitor)
        }

        fn struct_variant<V: de::Visitor<'d>>(
            self,
            fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value> {
            de::Deserializer::deserialize_tuple(self, fields.len(), visitor)
        }
    }

    impl<'d, 'a> de::EnumAccess<'d> for &'a mut IPCDeserializer<'d> {
        type Error = Error;
        type Variant = Self;

        fn variant_seed<V: de::DeserializeSeed<'d>>(self, seed: V) -> Result<(V::Value, Self)> {
            let variant_index = self.input.pop_front_byte()?;
            let variant = (variant_index as u32).into_deserializer();
            let v = de::DeserializeSeed::deserialize(seed, variant)?;
            Ok((v, self))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

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
            op: ToTask::FileReply(Ok(SysFd(5))),
        };
        let msg2 = MessageToSand::Task {
            task: VPid(39503),
            op: ToTask::FileReply(Err(Errno(2333))),
        };
        let msg3 = MessageToSand::Task {
            task: VPid(29862),
            op: ToTask::FileReply(Ok(SysFd(99999))),
        };
        let msg4 = MessageToSand::Task {
            task: VPid(125),
            op: ToTask::FileReply(Ok(SysFd(299))),
        };
        let mut buf = buffer::IPCBuffer::new();
        buf.push_back(&msg1).unwrap();
        buf.push_back(&msg2).unwrap();
        buf.push_back(&msg3).unwrap();
        buf.push_back(&msg4).unwrap();
        assert_eq!(buf.as_slice().bytes.len(), 32);
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
            0x00, 0x55, 0x99, 0x34, 0x12, 0x02, 0x00, 0x33, 0x99, 0x00, 0x11, 0x22, 0x33, 0x44,
            0x55, 0x22, 0x22, 0x56, 0x34, 0x88, 0x77, 0x66, 0x55,
        ],
        []
    );
    check!(
        sys_open_2,
        MessageFromSand::Task {
            task: VPid(0x22222222),
            op: FromTask::FileOpen {
                dir: Some(SysFd(0x11111111)),
                path: VString(VPtr(0x3333333333333333)),
                mode: 0x44444444,

                flags: 0x55555555
            }
        },
        MessageFromSand,
        [
            0x00, 0x22, 0x22, 0x22, 0x22, 0x02, 0x01, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33,
            0x33, 0x55, 0x55, 0x55, 0x55, 0x44, 0x44, 0x44, 0x44,
        ],
        [SysFd(0x11111111)]
    );
    check!(
        sys_open_reply_1,
        MessageToSand::Task {
            task: VPid(0x54555657),
            op: ToTask::FileReply(Ok(SysFd(42))),
        },
        MessageToSand,
        [0x00, 0x57, 0x56, 0x55, 0x54, 0x01, 0x00],
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
}
