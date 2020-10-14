use typenum::consts::*;
use core::future::Future;
use core::pin::Pin;
use pin_project::pin_project;
use typenum::marker_traits::Unsigned;
use heapless::{FnvIndexMap};
use crate::process::{Process, TaskFn};
use crate::protocol::{SysPid, VPid};

type PidLimit = U1024;

#[pin_project]
pub struct ProcessTable<'a, T: Future<Output=()>> {
    #[pin]
    table: [Option<Process<'a, T>>; PidLimit::USIZE],
    map_v_to_sys: FnvIndexMap<VPid, SysPid, PidLimit>,
    map_sys_to_v: FnvIndexMap<SysPid, VPid, PidLimit>,
    task_fn: TaskFn<'a, T>,
    next_vpid: VPid,
}

fn table_index_for_vpid(vpid: &VPid) -> Option<usize> {
    if vpid.0 >= 1 && vpid.0 <= PidLimit::U32 {
        Some((vpid.0 - 1) as usize)
    } else {
        None
    }
}

fn next_vpid_in_sequence(vpid: &VPid) -> VPid {
    match vpid {
        VPid(n) if *n == PidLimit::U32 => VPid(1),
        VPid(n) => VPid(n + 1),
    }
}

impl<'a, T: Future<Output=()>> ProcessTable<'a, T> {
    pub fn new(task_fn: TaskFn<'a, T>) -> Self {
        ProcessTable {
            task_fn,
            map_v_to_sys: FnvIndexMap::new(),
            map_sys_to_v: FnvIndexMap::new(),
            table: [None; PidLimit::USIZE],
            next_vpid: VPid(1),
        }
    }

    pub fn vpid_to_sys(&self, vpid: &VPid) -> Option<&SysPid> {
        self.map_v_to_sys.get(vpid)
    }

    pub fn syspid_to_v(&self, sys_pid: &SysPid) -> Option<&VPid> {
        self.map_sys_to_v.get(sys_pid)
    }

    fn allocate_vpid(&mut self) -> Option<VPid> {
        let mut result = None;
        for _ in 0 .. PidLimit::USIZE {
            let vpid = &self.next_vpid;
            let index = table_index_for_vpid(vpid).unwrap();
            if self.table[index].is_none() {
                result = Some(vpid.clone());
                break;
            } else {
                self.next_vpid = next_vpid_in_sequence(vpid);
            }
        }
        result
    }

    pub fn insert(&mut self, sys_pid: &SysPid) -> Option<VPid> {
        self.allocate_vpid().map(|vpid| {
            let index = table_index_for_vpid(&vpid).unwrap();
            let process = Process::new(self.task_fn);
            assert!(self.table[index].replace(process).is_none());
            assert_eq!(self.map_sys_to_v.insert(sys_pid.clone(), vpid.clone()), Ok(None));
            assert_eq!(self.map_v_to_sys.insert(vpid.clone(), sys_pid.clone()), Ok(None));
            Some(vpid)
        }).flatten()
    }

    pub fn get(&self, vpid: &VPid) -> Option<&Process<'a, T>> {
        table_index_for_vpid(vpid).map(move |index| (&self.table[index]).as_ref()).flatten()
    }

    pub fn get_mut(&mut self, pid: &VPid) -> Option<&mut Process<'a, T>> {
        table_index_for_vpid(pid).map(move |index| (&mut self.table[index]).as_mut()).flatten()
    }

    pub fn free(&mut self, pid: &VPid) -> Option<Process<'a, T>> {
        table_index_for_vpid(pid).map(move |index| self.table[index].take()).flatten()
    }
}
