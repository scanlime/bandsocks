use crate::{
    abi,
    protocol::{Errno, VPtr},
    remote::{
        file::RemoteFd,
        mem::{fault_or, write_padded_value},
        trampoline::Trampoline,
    },
};
use sc::nr;

impl<'q, 's, 't, 'r> Drop for Scratchpad<'q, 's, 't, 'r> {
    fn drop(&mut self) {
        panic!("leaking scratchpad")
    }
}

#[derive(Debug)]
pub struct Scratchpad<'q, 's, 't, 'r> {
    pub trampoline: &'r mut Trampoline<'q, 's, 't>,
    pub page_ptr: VPtr,
}

impl<'q, 's, 't, 'r> Scratchpad<'q, 's, 't, 'r> {
    pub async fn new(
        trampoline: &'r mut Trampoline<'q, 's, 't>,
    ) -> Result<Scratchpad<'q, 's, 't, 'r>, Errno> {
        let page_ptr = trampoline
            .mmap(
                VPtr(0),
                abi::PAGE_SIZE,
                abi::PROT_READ | abi::PROT_WRITE,
                abi::MAP_PRIVATE | abi::MAP_ANONYMOUS,
                &RemoteFd(0),
                0,
            )
            .await?;
        Ok(Scratchpad {
            trampoline,
            page_ptr,
        })
    }

    pub async fn free(self) -> Result<(), Errno> {
        self.trampoline
            .munmap(self.page_ptr, abi::PAGE_SIZE)
            .await?;
        core::mem::forget(self);
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn debug_loop(&mut self) -> ! {
        loop {
            RemoteFd(1)
                .write_bytes_exact(self, b"debug loop\n")
                .await
                .unwrap();
            self.sleep(&abi::TimeSpec::from_secs(10)).await.unwrap();
        }
    }

    pub async fn sleep(&mut self, duration: &abi::TimeSpec) -> Result<(), Errno> {
        fault_or(unsafe {
            write_padded_value(self.trampoline.stopped_task, self.page_ptr, duration)
        })?;
        let result = self
            .trampoline
            .syscall(nr::NANOSLEEP, &[self.page_ptr.0 as isize, 0])
            .await;
        if result == 0 {
            Ok(())
        } else {
            Err(Errno(result as i32))
        }
    }
}
