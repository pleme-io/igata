use colored::Colorize;

/// Print a build step starting.
pub fn print_build_start(builder_name: &str, builder_type: &str) {
    println!(
        "==> {}: {} build...",
        builder_name.bold().cyan(),
        builder_type,
    );
}

/// Print a build step completed.
pub fn print_build_done(builder_name: &str) {
    println!("==> {}: {}", builder_name.bold().cyan(), "Build finished.".green());
}

/// Print a build step failed.
pub fn print_build_error(builder_name: &str, err: &str) {
    eprintln!(
        "==> {}: {}",
        builder_name.bold().cyan(),
        format!("Build errored: {err}").red()
    );
}

/// Print a provisioner step.
pub fn print_provision(builder_name: &str, prov_type: &str, detail: &str) {
    println!(
        "    {}: Provisioning with {} — {detail}",
        builder_name.bold().cyan(),
        prov_type,
    );
}

/// Print a post-processor step.
pub fn print_post_process(builder_name: &str, pp_type: &str) {
    println!(
        "    {}: Running post-processor: {}",
        builder_name.bold().cyan(),
        pp_type,
    );
}

/// Print an artifact produced.
pub fn print_artifact(builder_name: &str, artifact_desc: &str) {
    println!(
        "==> {}: {} {}",
        builder_name.bold().cyan(),
        "Artifact:".green(),
        artifact_desc,
    );
}

/// Print a cleanup step.
pub fn print_cleanup(builder_name: &str) {
    println!(
        "    {}: Cleaning up...",
        builder_name.bold().cyan(),
    );
}

/// Print a validation error.
pub fn print_validation_error(msg: &str) {
    eprintln!("{} {msg}", "Error:".red().bold());
}

/// Print a validation warning.
pub fn print_validation_warning(msg: &str) {
    eprintln!("{} {msg}", "Warning:".yellow().bold());
}

/// Print machine-readable output line (Packer -machine-readable format).
#[allow(dead_code)]
pub fn print_machine_readable(timestamp: i64, target: &str, msg_type: &str, data: &str) {
    println!("{timestamp},{target},{msg_type},{data}");
}

/// Print a timestamped UI line (Packer -timestamp-ui format).
#[allow(dead_code)]
pub fn print_timestamped(msg: &str) {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    println!("[{now}] {msg}");
}
