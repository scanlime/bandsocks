use crate::process::Process;

use heapless::{Vec, FnvIndexMap};
use heapless::consts::*;

type PID_LIMIT = heapless::consts::U1024;

pub struct ProcessTable<T> {
    map_v_to_sys: FnvIndexMap<VPid, SysPid, PID_LIMIT>,
    map_sys_to_v: FnvIndexMap<SysPid, VPid, PID_LIMIT>,
    table: [Option<Process<T>, PID_LIMIT::USIZE - 1],
    next_vpid: VPid,
}

fn table_index_for_vpid(vpid: &VPid) -> Option<usize> {
    if vpid.0 > 0 && vpid.0 < PID_LIMIT::USIZE {
        Some((vpid.0 - 1) as usize)
    } else {
        None
    }
}

fn next_vpid_in_sequence(vpid: &VPid) -> VPid {
    match vpid {
        VPid(n) if n == PID_LIMIT::USIZE - 1 => VPid(1),
        VPid(n) => VPid(n + 1),
    }
}

impl<T> ProcessTable<T> {
    pub fn new() -> Self {
        ProcessTable {
            map_v_to_sys: FnvIndexMap::new(),
            map_sys_to_v: FnvIndexMap::new(),
            table: Vec::new(),
            next_vpid: VPid(1),
        }
    }

    pub fn vpid_to_sys(&self, vpid: &VPid) -> Option<&SysPid> {
        self.map_v_to_sys.get(vpid)
    }

    pub fn syspid_to_v(&self, sys_pid: &SysPid) -> Option<&VPid> {
        self.map_sys_to_v.get(vpid)
    }

    fn allocate_vpid(&mut self) -> Option<VPid> {
        let mut result = None;
        for _ in 0 .. PID_LIMIT::USIZE {
            let vpid = self.next_vpid;
            let index = table_index_for_vpid(&vpid).unwrap();
            if self.table[index].is_none() {
                result = Some(vpid);
                break;
            } else {
                self.next_vpid = next_vpid_in_sequence(vpid);
            }
        }
    }

    pub fn insert(&mut self, sys_pid: &SysPid, process: Process<T>) -> Option<VPid> {
        self.allocate_vpid().map(|vpid| {
            let index = table_index_for_vpid(vpid).unwrap();
            assert!(self.table[index].replace(process), None);
            assert!(self.map_sys_to_v.insert(sys_pid.clone(), vpid.clone()), Ok(None));
            assert!(self.map_v_to_sys.insert(vpid.clone(), sys_pid.clone()), Ok(None));
            Some(vpid)
        })
    }

    pub fn get(&self, vpid: &VPid) -> Option<&Process> {
        table_index_for_vpid(vpid).map(move |index| (&self.table[index]).as_ref()).flatten()
    }

    pub fn get_mut(&mut self, pid: VPid) -> Option<&mut Process> {
        table_index_for_vpid(pid).map(move |index| (&mut self.table[index]).as_mut()).flatten()
    }

    pub fn free(&mut self, pid: VPid) -> Option<Process> {
        table_index_for_vpid(pid).map(move |index| self.table[index].take()).flatten()
    }
}
