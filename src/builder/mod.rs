pub mod amazon_ebs;
pub mod docker;
pub mod null;
pub mod qemu;

use crate::traits::Registry;

/// Register all built-in builders.
pub fn register_all(registry: &mut Registry) {
    registry.register_builder("null", || Box::new(null::NullBuilder::new()));
    registry.register_builder("docker", || Box::new(docker::DockerBuilder::new()));
    registry.register_builder("qemu", || Box::new(qemu::QemuBuilder::new()));
    registry.register_builder("amazon-ebs", || {
        Box::new(amazon_ebs::AmazonEbsBuilder::new())
    });
}
