pub mod amazon_ebs;
pub mod amazon_import;
pub mod docker;
pub mod null;
pub mod qemu;

use crate::traits::Registry;
use serde_json::Value;
use std::collections::HashMap;

/// Read a string field out of a builder config map.
///
/// Shared across every builder that reads its config from the generic
/// `HashMap<String, Value>` the `Builder` trait passes around (Packer JSON /
/// shikumi YAML both flatten into this shape before a builder ever sees it).
pub(crate) fn get_str(config: &HashMap<String, Value>, key: &str) -> Option<String> {
    config.get(key).and_then(|v| v.as_str()).map(String::from)
}

/// Read a u64 field out of a builder config map. See [`get_str`].
pub(crate) fn get_u64(config: &HashMap<String, Value>, key: &str) -> Option<u64> {
    config.get(key).and_then(|v| v.as_u64())
}

/// Register all built-in builders.
pub fn register_all(registry: &mut Registry) {
    registry.register_builder("null", || Box::new(null::NullBuilder::new()));
    registry.register_builder("docker", || Box::new(docker::DockerBuilder::new()));
    registry.register_builder("qemu", || Box::new(qemu::QemuBuilder::new()));
    registry.register_builder("amazon-ebs", || {
        Box::new(amazon_ebs::AmazonEbsBuilder::new())
    });
    registry.register_builder("amazon-import", || {
        Box::new(amazon_import::AmazonImportBuilder::new())
    });
}
