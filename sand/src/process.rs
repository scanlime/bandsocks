// This code may not be used for any purpose. Be gay, do crime.

pub const PID_LIMIT: usize = 1024;

#[derive(Debug)]
pub struct SysPid(pub u32);

#[derive(Debug)]
pub struct VPid(pub u32);

pub struct ProcessTable {
    table: [Option<Process>; PID_LIMIT],
    next_potentially_unused_index: usize,
}

fn pid_to_index(pid: VPid) -> Option<usize> {
    if pid.0 >= 1 && pid.0 <= PID_LIMIT as u32 {
        Some(pid.0 as usize - 1)
    } else {
        None
    }
}

fn index_to_pid(index: usize) -> VPid {
    assert!(index < PID_LIMIT);
    VPid(1 + index as u32)
}

impl ProcessTable {
    pub fn new() -> Self {
        ProcessTable {
            table: [None; PID_LIMIT],
            next_potentially_unused_index: 0,
        }
    }

    pub fn get(&self, pid: VPid) -> Option<&Process> {
        pid_to_index(pid).map(move |index| (&self.table[index]).as_ref()).flatten()
    }

    pub fn get_mut(&mut self, pid: VPid) -> Option<&mut Process> {
        pid_to_index(pid).map(move |index| (&mut self.table[index]).as_mut()).flatten()
    }

    pub fn free(&mut self, pid: VPid) -> Option<Process> {
        pid_to_index(pid).map(move |index| self.table[index].take()).flatten()
    }

    fn unused_index(&self) -> Option<usize> {
        let mut counter = 0;
        let mut index = self.next_potentially_unused_index;
        while counter < PID_LIMIT && self.table[index].is_none() {
            counter += 1;
            index = (index + 1) % PID_LIMIT;
        }
        if self.table[index].is_none() {
            Some(index)
        } else {
            None
        }
    }
    
    pub fn allocate(&mut self, process: Process) -> Result<VPid, Process> {
        if let Some(index) = self.unused_index() {
            self.next_potentially_unused_index = (index + 1) % PID_LIMIT;
            self.table[index] = Some(process);
            Ok(index_to_pid(index))
        } else {
            Err(process)
        }
    }   
}

pub struct Process {
    pub sys_pid: SysPid,
    pub state: State,
}

pub enum State {
    Spawning,
}
