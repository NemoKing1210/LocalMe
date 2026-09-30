//! The listening socket.
//!
//! Binding is deliberately forgiving. A fixed port is what makes the Windows firewall prompt
//! a one-time event with a stable rule, so it is tried first; if it is taken — another
//! instance that failed to be caught by the single-instance guard, a stale process — an
//! ephemeral port is used instead and advertised through discovery, so a port conflict
//! degrades to "still works" rather than "does not start".

use std::net::SocketAddr;

use tokio::net::{TcpListener, TcpStream};

use crate::error::TransportError;

/// A bound listening socket.
#[derive(Debug)]
pub struct Listener {
    inner: TcpListener,
    port: u16,
    /// Whether we had to fall back to an ephemeral port.
    ephemeral: bool,
}

impl Listener {
    /// Binds the preferred port, falling back to an ephemeral one.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Io`] only if even an ephemeral bind fails, which means the
    /// machine has no usable IP stack for this application.
    pub async fn bind(preferred_port: u16) -> Result<Self, TransportError> {
        match TcpListener::bind(("0.0.0.0", preferred_port)).await {
            Ok(inner) => {
                let port = local_port(&inner)?;
                tracing::info!(port, "listening for peers");
                Ok(Self {
                    inner,
                    port,
                    ephemeral: false,
                })
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    preferred_port,
                    "preferred port is unavailable; falling back to an ephemeral port"
                );
                let inner = TcpListener::bind(("0.0.0.0", 0)).await?;
                let port = local_port(&inner)?;
                tracing::info!(port, "listening for peers on an ephemeral port");
                Ok(Self {
                    inner,
                    port,
                    ephemeral: true,
                })
            }
        }
    }

    /// The port actually bound.
    ///
    /// This, not the configured value, is what is announced through discovery.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Whether the preferred port was unavailable.
    #[must_use]
    pub fn is_ephemeral(&self) -> bool {
        self.ephemeral
    }

    /// Accepts the next inbound connection.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Io`] if the accept call itself fails. Per-connection errors
    /// are the connection task's problem, not the accept loop's: one bad client must not stop
    /// the listener.
    pub async fn accept(&self) -> Result<(TcpStream, SocketAddr), TransportError> {
        let (stream, address) = self.inner.accept().await?;
        Ok((stream, address))
    }
}

fn local_port(listener: &TcpListener) -> Result<u16, TransportError> {
    Ok(listener.local_addr()?.port())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_preferred_port_is_used_when_it_is_free() {
        // Port 0 asks the kernel for a free port, which is then reported back.
        let listener = Listener::bind(0).await.expect("bind");
        assert!(listener.port() > 0);
        assert!(!listener.is_ephemeral());
    }

    #[tokio::test]
    async fn a_taken_port_falls_back_to_an_ephemeral_one() {
        let first = Listener::bind(0).await.expect("bind");
        let taken = first.port();

        let second = Listener::bind(taken).await.expect("bind");
        assert!(
            second.is_ephemeral(),
            "the taken port must have been refused"
        );
        assert_ne!(second.port(), taken, "a different port must be in use");
        assert!(second.port() > 0);
    }

    #[tokio::test]
    async fn accepting_a_connection_reports_the_peer_address() {
        let listener = Listener::bind(0).await.expect("bind");
        let port = listener.port();

        let client = tokio::spawn(async move {
            tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .expect("connect")
        });

        let (accepted, address) = listener.accept().await.expect("accept");
        assert_eq!(address.ip().to_string(), "127.0.0.1");
        assert!(address.port() > 0);
        let _client = client.await.expect("client task");
        drop(accepted);
    }
}
