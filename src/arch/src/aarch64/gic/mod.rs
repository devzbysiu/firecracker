// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

mod dist_regs;
mod gicv2;
mod gicv3;
mod icc_regs;
mod redist_regs;

use std::{boxed::Box, result};

use kvm_ioctls::{DeviceFd, VmFd};

pub use self::dist_regs::{get_dist_regs, set_dist_regs};
pub use self::icc_regs::{get_icc_regs, set_icc_regs};
pub use self::redist_regs::{get_redist_regs, set_redist_regs};
use {super::layout, gicv2::GICv2, gicv3::GICv3};

/// Errors thrown while setting up the GIC.
#[derive(Debug)]
pub enum Error {
    /// Error while calling KVM ioctl for setting up the global interrupt controller.
    CreateGIC(kvm_ioctls::Error),
    /// Error while setting or getting device attributes for the GIC.
    DeviceAttribute(kvm_ioctls::Error, bool, u32),
}
type Result<T> = result::Result<T, Error>;

/// Trait for GIC devices.
pub trait GICDevice {
    /// Returns the file descriptor of the GIC device
    fn device_fd(&self) -> &DeviceFd;

    /// Returns an array with GIC device properties
    fn device_properties(&self) -> &[u64];

    /// Returns the number of vCPUs this GIC handles
    fn vcpu_count(&self) -> u64;

    /// Returns the fdt compatibility property of the device
    fn fdt_compatibility(&self) -> &str;

    /// Returns the maint_irq fdt property of the device
    fn fdt_maint_irq(&self) -> u32;

    /// Returns the GIC version of the device
    fn version() -> u32
    where
        Self: Sized;

    /// Create the GIC device object
    fn create_device(fd: DeviceFd, vcpu_count: u64) -> Box<dyn GICDevice>
    where
        Self: Sized;

    /// Setup the device-specific attributes
    fn init_device_attributes(gic_device: &dyn GICDevice) -> Result<()>
    where
        Self: Sized;

    /// Initialize a GIC device
    fn init_device(vm: &VmFd) -> Result<DeviceFd>
    where
        Self: Sized,
    {
        let mut gic_device = kvm_bindings::kvm_create_device {
            type_: Self::version(),
            fd: 0,
            flags: 0,
        };

        vm.create_device(&mut gic_device).map_err(Error::CreateGIC)
    }

    /// Set a GIC device attribute
    fn set_device_attribute(
        fd: &DeviceFd,
        group: u32,
        attr: u64,
        addr: u64,
        flags: u32,
    ) -> Result<()>
    where
        Self: Sized,
    {
        let attr = kvm_bindings::kvm_device_attr {
            group,
            attr,
            addr,
            flags,
        };
        fd.set_device_attr(&attr)
            .map_err(|e| Error::DeviceAttribute(e, true, group))?;

        Ok(())
    }

    /// Finalize the setup of a GIC device
    fn finalize_device(gic_device: &dyn GICDevice) -> Result<()>
    where
        Self: Sized,
    {
        /* On arm there are 3 types of interrupts: SGI (0-15), PPI (16-31), SPI (32-1020).
         * SPIs are used to signal interrupts from various peripherals accessible across
         * the whole system so these are the ones that we increment when adding a new virtio device.
         * KVM_DEV_ARM_VGIC_GRP_NR_IRQS sets the highest SPI number. Consequently, we will have a total
         * of `super::layout::IRQ_MAX - 32` usable SPIs in our microVM.
         */
        let nr_irqs: u32 = super::layout::IRQ_MAX;
        let nr_irqs_ptr = &nr_irqs as *const u32;
        Self::set_device_attribute(
            gic_device.device_fd(),
            kvm_bindings::KVM_DEV_ARM_VGIC_GRP_NR_IRQS,
            0,
            nr_irqs_ptr as u64,
            0,
        )?;

        /* Finalize the GIC.
         * See https://code.woboq.org/linux/linux/virt/kvm/arm/vgic/vgic-kvm-device.c.html#211.
         */
        Self::set_device_attribute(
            gic_device.device_fd(),
            kvm_bindings::KVM_DEV_ARM_VGIC_GRP_CTRL,
            u64::from(kvm_bindings::KVM_DEV_ARM_VGIC_CTRL_INIT),
            0,
            0,
        )?;

        Ok(())
    }

    /// Method to initialize the GIC device
    fn new(vm: &VmFd, vcpu_count: u64) -> Result<Box<dyn GICDevice>>
    where
        Self: Sized,
    {
        let vgic_fd = Self::init_device(vm)?;

        let device = Self::create_device(vgic_fd, vcpu_count);

        Self::init_device_attributes(device.as_ref())?;

        Self::finalize_device(device.as_ref())?;

        Ok(device)
    }
}

/// Create a GIC device.
///
/// It will try to create by default a GICv3 device. If that fails it will try
/// to fall-back to a GICv2 device.
pub fn create_gic(vm: &VmFd, vcpu_count: u64) -> Result<Box<dyn GICDevice>> {
    GICv3::new(vm, vcpu_count).or_else(|_| GICv2::new(vm, vcpu_count))
}

#[cfg(test)]
mod tests {

    use super::*;
    use kvm_ioctls::Kvm;

    #[test]
    fn test_create_gic() {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();
        assert!(create_gic(&vm, 1).is_ok());
    }
}