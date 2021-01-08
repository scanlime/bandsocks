use crate::{
    abi,
    process::{
        task::{TaskData, TaskMemManagement, TaskSocketPair},
        Process, TaskFn,
    },
    protocol::{Errno, SysPid, TracerSettings, VFile, VPid},
    remote::file::RemoteFd,
};
use alloc::{boxed::Box, rc::Rc, vec::Vec};
use core::{cell::RefCell, future::Future, pin::Pin};
use hashbrown::HashMap;

const PID_LIMIT: u32 = 1024 * 1024;

pub struct ProcessTable<'t, F: Future<Output = ()>> {
    table: Vec<Option<Pin<Box<Process<'t, F>>>>>,
    task_fn: TaskFn<'t, F>,
    map_sys_to_v: HashMap<SysPid, VPid>,
    next_vpid: VPid,
}

fn table_index_for_vpid(vpid: VPid) -> Option<usize> {
    if vpid.0 >= 1 && vpid.0 <= PID_LIMIT {
        Some((vpid.0 - 1) as usize)
    } else {
        None
    }
}

fn next_vpid_in_sequence(vpid: VPid) -> VPid {
    match vpid {
        VPid(n) if n == PID_LIMIT => VPid(1),
        VPid(n) => VPid(n + 1),
    }
}

impl<'t, F: Future<Output = ()>> ProcessTable<'t, F> {
    pub fn new(task_fn: TaskFn<'t, F>) -> Self {
        ProcessTable {
            map_sys_to_v: HashMap::new(),
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
        for _ in 0..PID_LIMIT {
            let vpid = self.next_vpid;
            let index = table_index_for_vpid(vpid).unwrap();
            if index >= self.table.len() || self.table[index].is_none() {
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
        file_table: FileTable,
    ) -> Option<VPid> {
        let vpid = self.allocate_vpid();
        vpid.map(move |vpid| {
            let task_data = TaskData {
                file_table,
                tracer_settings,
                sys_pid,
                vpid,
                parent,
                socket_pair,
                mm,
            };
            let index = table_index_for_vpid(vpid).unwrap();
            let min_table_len = index + 1;
            while self.table.len() < min_table_len {
                self.table.push(None);
            }

            let process = Box::pin(Process::new(self.task_fn, task_data));
            assert!(self.table[index].is_none());
            self.table[index] = Some(process);
            assert_eq!(self.map_sys_to_v.insert(sys_pid, vpid), None);
            Some(vpid)
        })
        .flatten()
    }

    pub fn get(&mut self, vpid: VPid) -> Option<&mut Pin<Box<Process<'t, F>>>> {
        let index = match table_index_for_vpid(vpid) {
            None => return None,
            Some(index) => index,
        };
        match &mut self.table[index] {
            None => None,
            Some(mut_ref) => Some(mut_ref),
        }
    }

    pub fn remove(&mut self, vpid: VPid) -> Option<SysPid> {
        let index = table_index_for_vpid(vpid).unwrap();
        let prev_sys_pid = self.table[index].as_ref().map(|process| process.sys_pid);
        self.table[index] = None;
        if let Some(sys_pid) = prev_sys_pid {
            assert_eq!(Some(vpid), self.map_sys_to_v.remove(&sys_pid));
        }
        prev_sys_pid
    }
}

#[derive(Debug, Clone)]
pub struct FileTable {
    table: Rc<RefCell<HashMap<RemoteFd, VFile>>>,
}

impl FileTable {
    pub fn new() -> Self {
        FileTable {
            table: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn open(&mut self, fd: RemoteFd, vfile: VFile) {
        self.table.borrow_mut().insert(fd, vfile);
    }

    pub fn close(&mut self, fd: &RemoteFd) {
        self.table.borrow_mut().remove(fd);
    }

    pub fn get(&self, fd: &RemoteFd) -> Result<VFile, Errno> {
        self.table
            .borrow()
            .get(fd)
            .cloned()
            .ok_or(Errno(-abi::EBADF))
    }

    pub fn dup(&mut self, src_fd: &RemoteFd, dest_fd: &RemoteFd) -> Result<(), Errno> {
        let vfile = self.get(src_fd)?;
        self.open(dest_fd.clone(), vfile);
        Ok(())
    }
}
