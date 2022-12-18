use super::listen::Responder;
use std::sync::Arc;

pub type EventHandler = Arc<dyn Fn(DiscoveryEvent) + Send + Sync + 'static>;

#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    ResponderFound(Arc<Responder>),
    ResponderLost(Arc<Responder>),
    ResponseUpdate {
        old: Arc<Responder>,
        new: Arc<Responder>,
    },
}
