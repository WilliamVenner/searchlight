use super::presence::Responder;
use std::sync::Arc;

pub type EventHandler = Arc<dyn Fn(DiscoveryEvent) + Send + Sync + 'static>;

#[derive(Debug, Clone)]
/// An event that can occur during discovery.
pub enum DiscoveryEvent {
	/// A new responder was found.
	ResponderFound(Arc<Responder>),

	/// A responder was lost.
	///
	/// This means the responder didn't respond to a query for a while, so we assume it's gone.
	ResponderLost(Arc<Responder>),

	/// A responder was updated.
	///
	/// This will occur even if the data in the DNS response is the same, it's up to you to detect whether the data has changed in the context of your application.
	ResponseUpdate {
		/// The previous state of the responder.
		old: Arc<Responder>,

		/// The new state of the responder.
		new: Arc<Responder>,
	},
}
