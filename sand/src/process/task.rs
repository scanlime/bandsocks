use crate::process::EventConsumer;

pub async fn task_fn<'a>(event_consumer: EventConsumer<'a>) {
    loop {
        println!("doing things???");
    }
}
