pub mod checksum;
pub mod compress;
pub mod manifest;
pub mod shell_local;

use crate::traits::Registry;

/// Register all built-in post-processors.
pub fn register_all(registry: &mut Registry) {
    registry.register_post_processor("manifest", || Box::new(manifest::ManifestPostProcessor));
    registry.register_post_processor("checksum", || Box::new(checksum::ChecksumPostProcessor));
    registry.register_post_processor("shell-local", || {
        Box::new(shell_local::ShellLocalPostProcessor)
    });
    registry.register_post_processor("compress", || Box::new(compress::CompressPostProcessor));
}
