use anyhow::{Context, Result};
use russh::client;
use ssh_key::PublicKey;
use std::path::Path;
use std::sync::Arc;

use crate::traits::{CommandOutput, Communicator};

/// SSH communicator using russh.
pub struct SshCommunicator {
    session: Arc<tokio::sync::Mutex<client::Handle<SshHandler>>>,
}

struct SshHandler;

#[async_trait::async_trait]
impl client::Handler for SshHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        // Accept all host keys (like Packer does by default)
        Ok(true)
    }
}

impl SshCommunicator {
    /// Connect to a remote host via SSH with password authentication.
    pub async fn connect_password(
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> Result<Self> {
        let config = client::Config::default();
        let mut session =
            client::connect(Arc::new(config), (host, port), SshHandler)
                .await
                .context("SSH connection failed")?;

        let authenticated = session
            .authenticate_password(username, password)
            .await
            .context("SSH password auth failed")?;

        if !authenticated {
            anyhow::bail!("SSH authentication failed for {username}@{host}:{port}");
        }

        Ok(Self {
            session: Arc::new(tokio::sync::Mutex::new(session)),
        })
    }

    /// Connect to a remote host via SSH with key-based authentication.
    pub async fn connect_key(
        host: &str,
        port: u16,
        username: &str,
        key_path: &Path,
    ) -> Result<Self> {
        let key_pair = russh_keys::load_secret_key(key_path, None)
            .context("failed to load SSH key")?;

        let config = client::Config::default();
        let mut session =
            client::connect(Arc::new(config), (host, port), SshHandler)
                .await
                .context("SSH connection failed")?;

        let authenticated = session
            .authenticate_publickey(username, Arc::new(key_pair))
            .await
            .context("SSH key auth failed")?;

        if !authenticated {
            anyhow::bail!("SSH key authentication failed for {username}@{host}:{port}");
        }

        Ok(Self {
            session: Arc::new(tokio::sync::Mutex::new(session)),
        })
    }
}

#[async_trait::async_trait]
impl Communicator for SshCommunicator {
    async fn upload(&self, src: &Path, dst: &str) -> Result<()> {
        let data = tokio::fs::read(src)
            .await
            .with_context(|| format!("failed to read {}", src.display()))?;

        let session = self.session.lock().await;
        let mut channel = session
            .channel_open_session()
            .await
            .context("failed to open SSH channel")?;

        channel
            .exec(true, format!("cat > {dst}"))
            .await
            .context("failed to exec upload command")?;

        channel.data(&data[..]).await.context("failed to send data")?;
        channel.eof().await.context("failed to send EOF")?;

        let _ = channel.wait().await;
        Ok(())
    }

    async fn download(&self, src: &str, dst: &Path) -> Result<()> {
        let session = self.session.lock().await;
        let mut channel = session
            .channel_open_session()
            .await
            .context("failed to open SSH channel")?;

        channel
            .exec(true, format!("cat {src}"))
            .await
            .context("failed to exec download command")?;

        let mut data = Vec::new();
        while let Some(msg) = channel.wait().await {
            if let russh::ChannelMsg::Data { data: chunk } = msg {
                data.extend_from_slice(&chunk);
            }
        }

        tokio::fs::write(dst, &data)
            .await
            .with_context(|| format!("failed to write {}", dst.display()))?;

        Ok(())
    }

    async fn exec(&self, command: &str) -> Result<CommandOutput> {
        let session = self.session.lock().await;
        let mut channel = session
            .channel_open_session()
            .await
            .context("failed to open SSH channel")?;

        channel
            .exec(true, command)
            .await
            .context("failed to exec command")?;

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0i32;

        while let Some(msg) = channel.wait().await {
            match msg {
                russh::ChannelMsg::Data { data } => {
                    stdout.push_str(&String::from_utf8_lossy(&data));
                }
                russh::ChannelMsg::ExtendedData { data, ext } if ext == 1 => {
                    stderr.push_str(&String::from_utf8_lossy(&data));
                }
                russh::ChannelMsg::ExitStatus { exit_status } => {
                    exit_code = exit_status as i32;
                }
                _ => {}
            }
        }

        Ok(CommandOutput {
            stdout,
            stderr,
            exit_code,
        })
    }
}
