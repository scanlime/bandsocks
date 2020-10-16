pub mod syscall;
pub mod table;
pub mod task;

use crate::{process::task::TaskData, protocol::ToSand};
use core::{
    future::Future,
    mem::replace,
    pin::Pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use heapless::spsc::{Consumer, Queue};
use pin_project::pin_project;
use typenum::consts::*;

pub type TaskFn<'t, F> = fn(EventSource<'t>, TaskData) -> F;

#[pin_project]
pub struct Process<'t, F: Future<Output = ()>> {
    #[pin]
    state: TaskState<'t, F>,
    #[pin]
    queue: EventQueue,
}

#[pin_project(project = TaskStateProj)]
enum TaskState<'t, F: Future<Output = ()>> {
    Initial(TaskFn<'t, F>, TaskData),
    Pollable(#[pin] F),
    None,
}

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
type EventConsumer<'q> = Consumer<'q, Event, EventQueueSize>;

pub struct EventSource<'q> {
    consumer: EventConsumer<'q>,
}

pub struct EventFuture<'q, 's> {
    source: &'s mut EventSource<'q>,
}

impl<'q, 's> Future for EventFuture<'q, 's> {
    type Output = Event;
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.source.consumer.dequeue() {
            None => Poll::Pending,
            Some(event) => Poll::Ready(event),
        }
    }
}

impl<'q, 's> EventSource<'q> {
    fn next(&'s mut self) -> EventFuture<'q, 's> {
        EventFuture { source: self }
    }
}

impl<'p, 't: 'p, F: Future<Output = ()>> Process<'t, F> {
    pub fn new(task_fn: TaskFn<'t, F>, task_data: TaskData) -> Self {
        Process {
            state: TaskState::Initial(task_fn, task_data),
            queue: EventQueue::new(),
        }
    }

    pub fn enqueue(self: Pin<&mut Self>, event: Event) -> Result<(), Event> {
        let mut producer = unsafe { self.project().queue.get_unchecked_mut().split().0 };
        producer.enqueue(event)
    }

    fn event_source(self: Pin<&'p mut Self>) -> EventSource<'t> {
        let queue = unsafe { self.project().queue.get_unchecked_mut() } as *mut EventQueue;
        let queue = unsafe { &mut *queue };
        let consumer = queue.split().1;
        EventSource { consumer }
    }

    pub fn poll(mut self: Pin<&'p mut Self>) -> Poll<()> {
        loop {
            let mut state_pin = self.as_mut().project().state;
            match state_pin.as_mut().project() {
                TaskStateProj::None => unreachable!(),
                TaskStateProj::Initial(_, _) => (),
                TaskStateProj::Pollable(future) => {
                    let raw_waker = RawWaker::new(core::ptr::null(), &WAKER_VTABLE);
                    let waker = unsafe { Waker::from_raw(raw_waker) };
                    let mut cx = Context::from_waker(&waker);
                    break future.poll(&mut cx);
                }
            }

            let prev_state = unsafe { replace(state_pin.get_unchecked_mut(), TaskState::None) };
            let next_state = match prev_state {
                TaskState::Initial(task_fn, task_data) => {
                    let event_source = self.as_mut().event_source();
                    TaskState::Pollable(task_fn(event_source, task_data))
                }
                _ => unreachable!(),
            };
            unsafe { self.as_mut().get_unchecked_mut().state = next_state };
        }
    }
}

const WAKER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(waker_clone, waker_wake, waker_wake_by_ref, waker_drop);

unsafe fn waker_clone(_: *const ()) -> RawWaker {
    panic!("waker_clone");
}

unsafe fn waker_wake(_: *const ()) {
    panic!("waker_wake");
}

unsafe fn waker_wake_by_ref(_: *const ()) {
    panic!("waker_wake_by_ref");
}

unsafe fn waker_drop(_: *const ()) {}
