//! Verified TLS for `bolt+s://` (the `tls` cargo feature): rustls with the
//! ring provider, trusting the platform store or a PEM file given by the
//! application. There is no option to skip verification.
use crate::{Error, Result};
use rustls::{
    pki_types::{pem::PemObject, CertificateDer, ServerName},
    ClientConfig, ClientConnection, RootCertStore, StreamOwned,
};
use std::{fs, net::TcpStream, path::Path, sync::Arc};

pub(crate) type TlsStream = StreamOwned<ClientConnection, TcpStream>;

pub(crate) fn client_config(trusted: Option<&Path>) -> Result<Arc<ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Tls(format!("TLS setup failed: {e}")))?;
    let config = match trusted {
        Some(path) => {
            let shown = path.display();
            let pem = fs::read(path).map_err(|e| {
                Error::Config(format!("cannot read trusted certificates {shown}: {e}"))
            })?;
            let mut roots = RootCertStore::empty();
            for cert in CertificateDer::pem_slice_iter(&pem) {
                let cert = cert.map_err(|e| {
                    Error::Config(format!("invalid PEM in trusted certificates {shown}: {e}"))
                })?;
                roots
                    .add(cert)
                    .map_err(|e| Error::Config(format!("unusable certificate in {shown}: {e}")))?;
            }
            if roots.is_empty() {
                return Err(Error::Config(format!(
                    "no certificates found in trusted certificates {shown}"
                )));
            }
            builder.with_root_certificates(roots).with_no_client_auth()
        }
        None => {
            use rustls_platform_verifier::BuilderVerifierExt;
            builder
                .with_platform_verifier()
                .map_err(|e| Error::Tls(format!("platform certificate verifier: {e}")))?
                .with_no_client_auth()
        }
    };
    Ok(Arc::new(config))
}

/// Complete the handshake before any Bolt bytes, so certificate and
/// handshake failures surface as [`Error::Tls`].
pub(crate) fn connect(
    config: Arc<ClientConfig>,
    host: &str,
    mut tcp: TcpStream,
) -> Result<TlsStream> {
    let name = ServerName::try_from(host.to_string())
        .map_err(|_| Error::Config(format!("invalid TLS server name {host}")))?;
    let mut conn = ClientConnection::new(config, name)
        .map_err(|e| Error::Tls(format!("TLS setup failed: {e}")))?;
    while conn.is_handshaking() {
        conn.complete_io(&mut tcp)
            .map_err(|e| Error::Tls(format!("TLS handshake with {host} failed: {e}")))?;
    }
    Ok(StreamOwned::new(conn, tcp))
}
