pub mod syscall;
pub mod table;
pub mod task;

use crate::protocol::ToSand;
use pin_project::pin_project;
use typenum::consts::*;
use heapless::spsc::{Queue, Consumer};
use core::future::Future;
use core::task::{Poll, Context};
use core::pin::Pin;

#[derive(Debug)]
pub enum Event {
    Message(ToSand),
    Signal(SigInfo),
}

#[derive(Debug)]
pub struct SigInfo {
    pub si_signo: u32,
    pub si_code: u32,
}

type EventQueueSize = U2;
type EventQueue = Queue<Event, EventQueueSize>;

pub struct EventSource<'a> {
    consumer: Consumer<'a, Event, EventQueueSize>
}

pub struct EventFuture<'a, 'b> {
    source: &'b mut EventSource<'a>
}

impl<'a, 'b> Future for EventFuture<'a, 'b> {
    type Output = Event;
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.source.consumer.dequeue() {
            None => Poll::Pending,
            Some(event) => Poll::Ready(event)
        }
    }
}

impl<'a, 'b> EventSource<'a> {
    fn next(&'b mut self) -> EventFuture<'a, 'b> {
        EventFuture { source: self }
    }
}

pub type TaskFn<'a, D, F> = fn(EventSource<'a>, &'a D) -> F;

#[pin_project]
pub struct Process<'a, D, F: Future<Output=()>> {
    #[pin] task: Option<F>,
    #[pin] queue: EventQueue,
    task_fn: TaskFn<'a, D, F>,
    task_data: D,
}

impl<'a, D, F: Future<Output=()>> Process<'a, D, F> {
    pub fn new(task_fn: TaskFn<'a, D, F>, task_data: D) -> Self {
        Process {
            task_fn,
            task_data,
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
        let task_data = &project.task_data;
        if task.as_mut().as_pin_mut().is_none() {
            let consumer = unsafe { project.queue.get_unchecked_mut().split().1 };
            let new_task = task_fn(EventSource { consumer }, task_data);
            unsafe { *task.as_mut().get_unchecked_mut() = Some(new_task) };
        }
        task.as_pin_mut().unwrap().poll(cx)
    }
}
