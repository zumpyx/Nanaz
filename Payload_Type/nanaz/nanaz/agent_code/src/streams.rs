pub trait StreamDriver {
    type Message;

    fn handle_inbound(&mut self, messages: Vec<Self::Message>);
    fn drain_outbound(&mut self) -> Vec<Self::Message>;
    fn requeue_outbound_front(&mut self, messages: Vec<Self::Message>);
    fn wants_fast_poll(&self) -> bool;
}
