pub mod breakpoint;
pub mod file;
pub mod shell;
pub mod shell_local;

use crate::traits::Registry;

/// Register all built-in provisioners.
pub fn register_all(registry: &mut Registry) {
    registry.register_provisioner("shell", || Box::new(shell::ShellProvisioner));
    registry.register_provisioner("file", || Box::new(file::FileProvisioner));
    registry.register_provisioner("shell-local", || {
        Box::new(shell_local::ShellLocalProvisioner)
    });
    registry.register_provisioner("breakpoint", || {
        Box::new(breakpoint::BreakpointProvisioner)
    });
}
