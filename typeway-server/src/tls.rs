//! TLS support for typeway servers.
//!
//! Enabled with `feature = "tls"`. Provides [`TlsConfig`] for loading
//! certificates and [`Server::serve_tls`](crate::server::Server::serve_tls) for HTTPS.
//!
//! # Example
//!
//! ```ignore
//! use typeway_server::tls::TlsConfig;
//!
//! let tls = TlsConfig::from_pem("cert.pem", "key.pem")?;
//!
//! Server::<API>::new(handlers)
//!     .serve_tls("0.0.0.0:443".parse()?, tls)
//!     .await?;
//! ```

use std::io;
use std::path::Path;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// TLS configuration loaded from PEM files.
pub struct TlsConfig {
    acceptor: TlsAcceptor,
}

impl TlsConfig {
    /// Load TLS config from PEM certificate and private key files.
    pub fn from_pem(cert_path: impl AsRef<Path>, key_path: impl AsRef<Path>) -> io::Result<Self> {
        let cert_data = std::fs::read(cert_path.as_ref())?;
        let key_data = std::fs::read(key_path.as_ref())?;

        let certs: Vec<_> =
            rustls_pemfile::certs(&mut &cert_data[..]).collect::<Result<Vec<_>, _>>()?;
        let key = rustls_pemfile::private_key(&mut &key_data[..])
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no private key found"))?;

        let config = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok(TlsConfig {
            acceptor: TlsAcceptor::from(Arc::new(config)),
        })
    }

    pub(crate) fn acceptor(&self) -> &TlsAcceptor {
        &self.acceptor
    }
}

/// Serve HTTPS with TLS.
///
/// Called internally by `Server::serve_tls`.
pub(crate) async fn serve_tls_loop(
    listener: TcpListener,
    tls: TlsConfig,
    make_service: impl Fn() -> hyper_util::service::TowerToHyperService<crate::router::RouterService>
        + Send
        + Sync
        + 'static,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    loop {
        let (stream, addr) = listener.accept().await?;
        let acceptor = tls.acceptor().clone();
        let svc = make_service();

        tokio::spawn(async move {
            match acceptor.accept(stream).await {
                Ok(tls_stream) => {
                    let io = hyper_util::rt::TokioIo::new(tls_stream);
                    if let Err(e) = hyper_util::server::conn::auto::Builder::new(
                        hyper_util::rt::TokioExecutor::new(),
                    )
                    .serve_connection(io, svc)
                    .await
                    {
                        tracing::debug!("TLS connection closed: {e}");
                    }
                }
                Err(e) => {
                    tracing::debug!("TLS handshake failed from {addr}: {e}");
                }
            }
        });
    }
}
