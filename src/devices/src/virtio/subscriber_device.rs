use std::os::unix::io::AsRawFd;

use crate::virtio::device::VirtioDevice;
use ::utils::eventfd::EventFd;
use logger::{debug, error};
use polly::event_manager::{EventManager, Subscriber};

/// Trait for virtio devices which also have the ability to respond to I/O event readiness.
///
/// It handles device activation event. The default implementation does four things:
/// - consumes activate event
/// - gets events the device is interested in
/// - registers self as a subscriber of each of those events
/// - unregisters self as a subcriber of activate event as it just got activated
pub trait SubscriberVirtioDevice: VirtioDevice + Subscriber {
    /// Callback called when activate event appears.
    fn process_activate_event(&self, event_manager: &mut EventManager) {
        debug!("Device type {}: activate event", self.device_type());
        if let Err(e) = self.activate_fd().read() {
            error!(
                "Failed to consume device type {} activate event: {:?}",
                self.device_type(),
                e
            );
        }
        let activate_fd = self.activate_fd().as_raw_fd();
        // The subscriber must exist as we previously registered activate_evt via
        // `interest_list()`.
        let self_subscriber = match event_manager.subscriber(activate_fd) {
            Ok(subscriber) => subscriber,
            Err(e) => {
                error!(
                    "Failed to process device type {} activate evt: {:?}",
                    self.device_type(),
                    e
                );
                return;
            }
        };

        // Interest list changes when the device is activated.
        let interest_list = self.interest_list();
        for event in interest_list {
            event_manager
                .register(event.data() as i32, event, self_subscriber.clone())
                .unwrap_or_else(|e| {
                    error!(
                        "Failed to register device type {} events: {:?}",
                        self.device_type(),
                        e
                    );
                });
        }

        event_manager.unregister(activate_fd).unwrap_or_else(|e| {
            error!(
                "Failed to unregister device type {} activate evt: {:?}",
                self.device_type(),
                e
            );
        });
    }

    /// Returns a reference to `EventFd` of an activate event.
    fn activate_fd(&self) -> &EventFd;
}
