pub mod syscall;
pub mod table;
pub mod task;

use crate::protocol::ToSand;
use crate::process::task::TaskData;
use pin_project::pin_project;
use typenum::consts::*;
use heapless::spsc::{Queue, Consumer};
use core::future::Future;
use core::task::{Poll, Context};
use core::pin::Pin;
use core::marker::PhantomData;

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

pub type TaskFn<'t, F> = fn(EventSource<'t>, TaskData) -> F;

#[pin_project]
pub struct Process<'t, F: Future<Output=()>> {
    #[pin] future: Option<F>,
    #[pin] queue: EventQueue,
    task_fn: TaskFn<'t, F>,
    task_data: TaskData,
    task_queue: PhantomData<&'t EventQueue>
}

impl<'t, 'p: 't, F: Future<Output=()>> Process<'t, F> {
    pub fn new(task_fn: TaskFn<'t, F>, task_data: TaskData) -> Self {
        Process {
            task_fn,
            task_data,
            task_queue: PhantomData,
            future: None,
            queue: EventQueue::new(),
        }
    }

    pub fn enqueue(self: Pin<&mut Self>, event: Event) -> Result<(), Event> {
        let mut producer = unsafe { self.project().queue.get_unchecked_mut().split().0 };
        producer.enqueue(event)
    }

    pub fn poll(self: Pin<&'p mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let project = self.project();
        let mut future = project.future;
        let task_fn = project.task_fn;
        let task_data = project.task_data.clone();
        if future.as_mut().as_pin_mut().is_none() {
            let consumer = unsafe { project.queue.get_unchecked_mut().split().1 };
            let fut = task_fn(EventSource { consumer }, task_data);
            unsafe { *future.as_mut().get_unchecked_mut() = Some(fut) };
        }
        future.as_pin_mut().unwrap().poll(cx)
    }
}
