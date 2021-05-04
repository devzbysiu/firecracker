use std::os::unix::io::AsRawFd;

use crate::virtio::device::VirtioDevice;
use ::utils::eventfd::EventFd;
use logger::{debug, error};
use polly::event_manager::{EventManager, Subscriber};

pub trait SubscriberVirtioDevice: VirtioDevice + Subscriber {
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

    fn activate_fd(&self) -> &EventFd;
}
