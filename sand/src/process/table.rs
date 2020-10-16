use crate::{
    process::{task::TaskData, Process, TaskFn},
    protocol::{SysPid, VPid},
};
use core::{future::Future, pin::Pin};
use heapless::FnvIndexMap;
use pin_project::pin_project;
use typenum::{consts::*, marker_traits::Unsigned};

type PidLimit = U1024;

#[pin_project]
pub struct ProcessTable<'t, F: Future<Output = ()>> {
    #[pin]
    table: [Option<Process<'t, F>>; PidLimit::USIZE],
    task_fn: TaskFn<'t, F>,
    map_v_to_sys: FnvIndexMap<VPid, SysPid, PidLimit>,
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
            map_v_to_sys: FnvIndexMap::new(),
            map_sys_to_v: FnvIndexMap::new(),
            table: [None; PidLimit::USIZE],
            next_vpid: VPid(1),
            task_fn,
        }
    }

    pub fn vpid_to_sys(&self, vpid: VPid) -> Option<SysPid> {
        self.map_v_to_sys.get(&vpid).copied()
    }

    pub fn syspid_to_v(&self, sys_pid: SysPid) -> Option<VPid> {
        self.map_sys_to_v.get(&sys_pid).copied()
    }

    fn allocate_vpid(self: Pin<&mut Self>) -> Option<VPid> {
        let mut result = None;
        let project = self.project();
        for _ in 0..PidLimit::USIZE {
            let vpid = *project.next_vpid;
            let index = table_index_for_vpid(vpid).unwrap();
            if project.table[index].is_none() {
                result = Some(vpid);
                break;
            } else {
                *project.next_vpid = next_vpid_in_sequence(vpid);
            }
        }
        result
    }

    pub fn insert(mut self: Pin<&mut Self>, sys_pid: SysPid) -> Option<VPid> {
        let vpid = self.as_mut().allocate_vpid();
        vpid.map(move |vpid| {
            let task_data = TaskData { sys_pid, vpid };
            let project = self.project();
            let index = table_index_for_vpid(vpid).unwrap();
            let process = Process::new(*project.task_fn, task_data);
            unsafe {
                let table = project.table.get_unchecked_mut();
                let prev = table[index].replace(process);
                assert!(prev.is_none());
            }
            assert_eq!(project.map_sys_to_v.insert(sys_pid, vpid), Ok(None));
            assert_eq!(project.map_v_to_sys.insert(vpid, sys_pid), Ok(None));
            Some(vpid)
        })
        .flatten()
    }

    pub fn get(self: Pin<&mut Self>, vpid: VPid) -> Option<Pin<&mut Process<'t, F>>> {
        table_index_for_vpid(vpid)
            .map(move |index| {
                let table_pin = self.project().table;
                unsafe {
                    let table = table_pin.get_unchecked_mut();
                    Pin::new_unchecked(&mut table[index]).as_pin_mut()
                }
            })
            .flatten()
    }

    pub fn free(self: Pin<&mut Self>, vpid: VPid) {
        table_index_for_vpid(vpid).map(move |index| {
            let table_pin = self.project().table;
            unsafe {
                let table = table_pin.get_unchecked_mut();
                table[index] = None;
            }
        });
    }
}
