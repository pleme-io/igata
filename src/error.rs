use std::path::PathBuf;

/// All errors that can occur during igata operations.
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum Error {
    #[error("template parse error in {path}: {detail}")]
    TemplateParse { path: PathBuf, detail: String },

    #[error("variable '{name}' is required but has no value")]
    VariableRequired { name: String },

    #[error("interpolation error: {0}")]
    Interpolation(String),

    #[error("unknown builder type: {0}")]
    UnknownBuilder(String),

    #[error("unknown provisioner type: {0}")]
    UnknownProvisioner(String),

    #[error("unknown post-processor type: {0}")]
    UnknownPostProcessor(String),

    #[error("builder '{name}' failed: {detail}")]
    BuildFailed { name: String, detail: String },

    #[error("provisioner failed: {0}")]
    ProvisionFailed(String),

    #[error("post-processor failed: {0}")]
    PostProcessFailed(String),

    #[error("communicator error: {0}")]
    Communicator(String),

    #[error("SSH connection failed: {0}")]
    Ssh(String),

    #[error("Docker error: {0}")]
    Docker(String),

    #[error("AWS error: {0}")]
    Aws(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("command failed: {cmd} (exit code {code:?}): {stderr}")]
    CommandFailed {
        cmd: String,
        code: Option<i32>,
        stderr: String,
    },

    #[error("timeout waiting for {what} after {seconds}s")]
    Timeout { what: String, seconds: u64 },

    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_template_parse() {
        let e = Error::TemplateParse {
            path: PathBuf::from("/tmp/t.json"),
            detail: "unexpected EOF".into(),
        };
        assert_eq!(
            e.to_string(),
            "template parse error in /tmp/t.json: unexpected EOF"
        );
    }

    #[test]
    fn test_error_display_variable_required() {
        let e = Error::VariableRequired {
            name: "secret".into(),
        };
        assert_eq!(e.to_string(), "variable 'secret' is required but has no value");
    }

    #[test]
    fn test_error_display_unknown_builder() {
        let e = Error::UnknownBuilder("fake".into());
        assert_eq!(e.to_string(), "unknown builder type: fake");
    }

    #[test]
    fn test_error_display_unknown_provisioner() {
        let e = Error::UnknownProvisioner("fake".into());
        assert_eq!(e.to_string(), "unknown provisioner type: fake");
    }

    #[test]
    fn test_error_display_unknown_post_processor() {
        let e = Error::UnknownPostProcessor("fake".into());
        assert_eq!(e.to_string(), "unknown post-processor type: fake");
    }

    #[test]
    fn test_error_display_build_failed() {
        let e = Error::BuildFailed {
            name: "docker".into(),
            detail: "pull failed".into(),
        };
        assert_eq!(e.to_string(), "builder 'docker' failed: pull failed");
    }

    #[test]
    fn test_error_display_command_failed() {
        let e = Error::CommandFailed {
            cmd: "ssh".into(),
            code: Some(255),
            stderr: "connection refused".into(),
        };
        assert_eq!(
            e.to_string(),
            "command failed: ssh (exit code Some(255)): connection refused"
        );
    }

    #[test]
    fn test_error_display_command_failed_no_code() {
        let e = Error::CommandFailed {
            cmd: "killed".into(),
            code: None,
            stderr: "signal".into(),
        };
        assert!(e.to_string().contains("None"));
    }

    #[test]
    fn test_error_display_timeout() {
        let e = Error::Timeout {
            what: "SSH".into(),
            seconds: 300,
        };
        assert_eq!(e.to_string(), "timeout waiting for SSH after 300s");
    }

    #[test]
    fn test_error_display_other() {
        let e = Error::Other("misc".into());
        assert_eq!(e.to_string(), "misc");
    }

    #[test]
    fn test_error_display_ssh() {
        let e = Error::Ssh("key rejected".into());
        assert_eq!(e.to_string(), "SSH connection failed: key rejected");
    }

    #[test]
    fn test_error_display_docker() {
        let e = Error::Docker("daemon not running".into());
        assert_eq!(e.to_string(), "Docker error: daemon not running");
    }

    #[test]
    fn test_error_display_aws() {
        let e = Error::Aws("invalid credentials".into());
        assert_eq!(e.to_string(), "AWS error: invalid credentials");
    }

    #[test]
    fn test_error_display_validation() {
        let e = Error::Validation("no builders".into());
        assert_eq!(e.to_string(), "validation error: no builders");
    }

    #[test]
    fn test_error_display_interpolation() {
        let e = Error::Interpolation("undefined var".into());
        assert_eq!(e.to_string(), "interpolation error: undefined var");
    }

    #[test]
    fn test_error_display_provision_failed() {
        let e = Error::ProvisionFailed("script error".into());
        assert_eq!(e.to_string(), "provisioner failed: script error");
    }

    #[test]
    fn test_error_display_post_process_failed() {
        let e = Error::PostProcessFailed("compress error".into());
        assert_eq!(e.to_string(), "post-processor failed: compress error");
    }

    #[test]
    fn test_error_display_communicator() {
        let e = Error::Communicator("upload failed".into());
        assert_eq!(e.to_string(), "communicator error: upload failed");
    }
}
