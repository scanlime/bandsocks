pub mod syscall;
pub mod table;
pub mod task;

use crate::protocol::ToSand;
use pin_project::pin_project;
use typenum::consts::*;
use heapless::spsc::{Queue, Producer, Consumer};
use core::future::Future;
use core::task::{Poll, Context};
use core::pin::Pin;

pub enum Event {
    Message(ToSand),
    Signal(u32),
}

type EventQueueSize = U2;
type EventQueue = Queue<Event, EventQueueSize>;
pub type EventConsumer<'a> = Consumer<'a, Event, EventQueueSize>;
pub type TaskFn<'a, T> = fn(EventConsumer<'a>) -> T;

#[pin_project]
pub struct Process<'a, T: Future<Output=()>> {
    #[pin]
    task: Option<T>,
    #[pin]
    queue: EventQueue,
    task_fn: TaskFn<'a, T>,
}

impl<'a, T: Future<Output=()>> Process<'a, T> {
    pub fn new(task_fn: TaskFn<'a, T>) -> Self {
        Process {
            task_fn,
            task: None,
            queue: EventQueue::new()
        }
    }

    pub fn enqueue(self: Pin<&'a mut Self>, event: Event) -> Result<(), Event> {
        let mut producer = unsafe { self.project().queue.get_unchecked_mut().split().0 };
        producer.enqueue(event)
    }

    pub fn poll(self: Pin<&'a mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let project = self.project();
        let mut task = project.task;
        let task_fn = project.task_fn;
        if task.as_mut().as_pin_mut().is_none() {
            let consumer = unsafe { project.queue.get_unchecked_mut().split().1 };
            let new_task = task_fn(consumer);
            unsafe { *task.as_mut().get_unchecked_mut() = Some(new_task) };
        }
        task.as_pin_mut().unwrap().poll(cx)
    }
}
