use crate::{
    nolibc,
    process::{
        task::{TaskData, TaskMemManagement, TaskSocketPair},
        Process, TaskFn,
    },
    protocol::{SysPid, TracerSettings, VPid},
};
use core::{future::Future, mem::size_of, pin::Pin};
use heapless::{FnvIndexMap, Vec};
use typenum::{consts::*, marker_traits::Unsigned};

type PidLimit = U32768;

pub struct ProcessTable<'t, F: Future<Output = ()>> {
    table: Vec<*mut Process<'t, F>, PidLimit>,
    task_fn: TaskFn<'t, F>,
    map_sys_to_v: FnvIndexMap<SysPid, VPid, PidLimit>,
    next_vpid: VPid,
}

fn table_index_for_vpid(vpid: VPid) -> Option<usize> {
    if vpid.0 >= 1 && vpid.0 <= PidLimit::U32 {
        Some((vpid.0 - 1) as usize)
    } else {
        None
    }
}

fn next_vpid_in_sequence(vpid: VPid) -> VPid {
    match vpid {
        VPid(n) if n == PidLimit::U32 => VPid(1),
        VPid(n) => VPid(n + 1),
    }
}

impl<'t, F: Future<Output = ()>> ProcessTable<'t, F> {
    pub fn new(task_fn: TaskFn<'t, F>) -> Self {
        ProcessTable {
            map_sys_to_v: FnvIndexMap::new(),
            table: Vec::new(),
            next_vpid: VPid(1),
            task_fn,
        }
    }

    pub fn syspid_to_v(&self, sys_pid: SysPid) -> Option<VPid> {
        self.map_sys_to_v.get(&sys_pid).copied()
    }

    fn allocate_vpid(&mut self) -> Option<VPid> {
        let mut result = None;
        for _ in 0..PidLimit::USIZE {
            let vpid = self.next_vpid;
            let index = table_index_for_vpid(vpid).unwrap();
            if index >= self.table.len() || self.table[index].is_null() {
                result = Some(vpid);
                break;
            } else {
                self.next_vpid = next_vpid_in_sequence(vpid);
            }
        }
        result
    }

    pub fn insert(
        &mut self,
        tracer_settings: TracerSettings,
        sys_pid: SysPid,
        parent: Option<VPid>,
        socket_pair: TaskSocketPair,
        mm: TaskMemManagement,
    ) -> Option<VPid> {
        let vpid = self.allocate_vpid();
        vpid.map(move |vpid| {
            let task_data = TaskData {
                tracer_settings,
                sys_pid,
                vpid,
                parent,
                socket_pair,
                mm,
            };
            let index = table_index_for_vpid(vpid).unwrap();
            let min_table_len = index + 1;
            if self.table.len() < min_table_len {
                self.table
                    .resize(min_table_len, core::ptr::null_mut())
                    .unwrap();
            }

            let process = Process::new(self.task_fn, task_data);
            let process_ptr =
                nolibc::alloc_pages(size_of::<Process<'t, F>>()) as *mut Process<'t, F>;
            unsafe {
                process_ptr.write(process);
            }
            assert!(self.table[index].is_null());
            self.table[index] = process_ptr;
            assert_eq!(self.map_sys_to_v.insert(sys_pid, vpid), Ok(None));
            Some(vpid)
        })
        .flatten()
    }

    pub fn get(&mut self, vpid: VPid) -> Option<Pin<&mut Process<'t, F>>> {
        table_index_for_vpid(vpid)
            .map(move |index| {
                let process_ptr = self.table[index];
                if process_ptr.is_null() {
                    None
                } else {
                    unsafe { Some(Pin::new_unchecked(&mut *process_ptr)) }
                }
            })
            .flatten()
    }

    pub fn remove(&mut self, vpid: VPid) -> Option<SysPid> {
        let index = table_index_for_vpid(vpid).unwrap();
        let prev = self.table[index];
        self.table[index] = core::ptr::null_mut();
        if prev.is_null() {
            return None;
        }
        unsafe {
            let process = &mut *prev;
            let sys_pid = process.sys_pid;
            assert_eq!(Some(vpid), self.map_sys_to_v.remove(&sys_pid));
            core::ptr::drop_in_place(prev);
            nolibc::free_pages(prev as usize, size_of::<Process<'t, F>>());
            Some(sys_pid)
        }
    }
}
