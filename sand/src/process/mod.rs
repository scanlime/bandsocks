macro_rules! ipc_call {
    ( $task:expr, $op:expr, $reply:pat, $result:expr ) => {{
        $task.msg.send($op);
        match $task.events.next().await {
            crate::process::Event::Message($reply) => $result,
            other => panic!(
                "unexpected ipc_call reply, task={:x?} op={:x?}, received: {:x?}",
                $task, $op, other
            ),
        }
    }};
}

pub mod loader;
pub mod maps;
pub mod stack;
pub mod syscall;
pub mod table;
pub mod task;

use crate::{
    process::task::TaskData,
    protocol::{FromTask, ToTask},
};
use core::{
    future::Future,
    mem::replace,
    pin::Pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};
use heapless::spsc::{Consumer, Producer, Queue};
use pin_project::pin_project;
use typenum::consts::*;

pub type TaskFn<'t, F> = fn(EventSource<'t>, MessageSender<'t>, TaskData) -> F;

#[pin_project]
pub struct Process<'t, F: Future<Output = ()>> {
    #[pin]
    state: TaskState<'t, F>,
    #[pin]
    event_queue: EventQueue,
    #[pin]
    outbox_queue: OutboxQueue,
}

#[pin_project(project = TaskStateProj)]
enum TaskState<'t, F: Future<Output = ()>> {
    Initial(TaskFn<'t, F>, TaskData),
    Pollable(#[pin] F),
    None,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Event {
    Message(ToTask),
    Signal { sig: u32, code: u32, status: u32 },
}

type EventQueueSize = U2;
type EventQueue = Queue<Event, EventQueueSize>;
type EventConsumer<'q> = Consumer<'q, Event, EventQueueSize>;

type OutboxQueueSize = U4;
type OutboxQueue = Queue<FromTask, OutboxQueueSize>;
type OutboxProducer<'q> = Producer<'q, FromTask, OutboxQueueSize>;

pub struct MessageSender<'q> {
    producer: OutboxProducer<'q>,
}

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
    pub fn next(&'s mut self) -> EventFuture<'q, 's> {
        EventFuture { source: self }
    }
}

impl<'q> MessageSender<'q> {
    pub fn send(&mut self, message: FromTask) {
        self.producer.enqueue(message).expect("message outbox full");
    }
}

impl<'p, 't: 'p, F: Future<Output = ()>> Process<'t, F> {
    pub fn new(task_fn: TaskFn<'t, F>, task_data: TaskData) -> Self {
        Process {
            state: TaskState::Initial(task_fn, task_data),
            event_queue: EventQueue::new(),
            outbox_queue: OutboxQueue::new(),
        }
    }

    pub fn send_event(self: Pin<&mut Self>, event: Event) -> Result<(), Event> {
        let mut producer = unsafe { self.project().event_queue.get_unchecked_mut().split().0 };
        producer.enqueue(event)
    }

    pub fn check_outbox(self: Pin<&mut Self>) -> Option<FromTask> {
        let mut consumer = unsafe { self.project().outbox_queue.get_unchecked_mut().split().1 };
        consumer.dequeue()
    }

    fn event_source(self: Pin<&'p mut Self>) -> EventSource<'t> {
        let queue = unsafe { self.project().event_queue.get_unchecked_mut() } as *mut EventQueue;
        let queue = unsafe { &mut *queue };
        let consumer = queue.split().1;
        EventSource { consumer }
    }

    fn message_sender(self: Pin<&'p mut Self>) -> MessageSender<'t> {
        let queue = unsafe { self.project().outbox_queue.get_unchecked_mut() } as *mut OutboxQueue;
        let queue = unsafe { &mut *queue };
        let producer = queue.split().0;
        MessageSender { producer }
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
                    let message_sender = self.as_mut().message_sender();
                    TaskState::Pollable(task_fn(event_source, message_sender, task_data))
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
