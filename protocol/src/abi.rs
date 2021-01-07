//! Definitions that overlap between the kernel ABI and the sand IPC protocol

use core::fmt;

// getdents(2)
#[derive(Debug)]
#[repr(C)]
pub struct DirentHeader {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: u8,
}

// POSIX dirent.h or linux fs_types.h
pub const DT_UNKNOWN: u8 = 0;
pub const DT_FIFO: u8 = 1;
pub const DT_CHR: u8 = 2;
pub const DT_DIR: u8 = 4;
pub const DT_BLK: u8 = 6;
pub const DT_REG: u8 = 8;
pub const DT_LNK: u8 = 10;
pub const DT_SOCK: u8 = 12;
pub const DT_WHT: u8 = 14;

// st_mode, as described in inode(7)
pub const S_IFMT: u32 = 0o170000;
pub const S_IFSOCK: u32 = 0o140000;
pub const S_IFLNK: u32 = 0o120000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFBLK: u32 = 0o060000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFCHR: u32 = 0o020000;
pub const S_IFIFO: u32 = 0o010000;

#[derive(PartialEq, Eq, Ord, PartialOrd, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct Syscall {
    pub nr: isize,
    pub args: [isize; 6],
    pub ret: isize,
    pub ip: usize,
    pub sp: usize,
}

impl fmt::Debug for Syscall {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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

impl fmt::Debug for UserRegs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
