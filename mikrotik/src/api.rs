use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

// ── stream abstraction ────────────────────────────────────────────────────────

trait RosIo: Read + Write {}
impl<T: Read + Write> RosIo for T {}

// ── TLS: no-verify verifier (MikroTik uses self-signed certs by default) ──────

mod tls {
    use rustls::client::danger::{
        HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
    };
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
    use std::sync::Arc;

    #[derive(Debug)]
    struct NoVerifier;

    impl ServerCertVerifier for NoVerifier {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    pub fn no_verify_config() -> Arc<ClientConfig> {
        let config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();
        Arc::new(config)
    }
}

// ── RosSession ────────────────────────────────────────────────────────────────

pub struct RosSession {
    stream: Box<dyn RosIo>,
}

impl RosSession {
    pub fn connect(host: &str, port: u16, ssl: bool) -> Result<Self> {
        let tcp = TcpStream::connect((host, port))
            .with_context(|| format!("connect to RouterOS API at {host}:{port}"))?;
        tcp.set_read_timeout(Some(Duration::from_secs(10)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(10)))?;

        if ssl {
            let config = tls::no_verify_config();
            let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
                .with_context(|| format!("invalid hostname for TLS: {host}"))?;
            let conn = rustls::ClientConnection::new(config, server_name)
                .context("create TLS client connection")?;
            Ok(Self { stream: Box::new(rustls::StreamOwned::new(conn, tcp)) })
        } else {
            Ok(Self { stream: Box::new(tcp) })
        }
    }

    pub fn login(&mut self, user: &str, password: &str) -> Result<()> {
        self.sentence(&["/login", &format!("=name={user}"), &format!("=password={password}")])?;
        let reply = self.read_reply()?;
        match reply.first().map(|s| s.as_str()) {
            Some("!done") => Ok(()),
            Some("!trap") => {
                let msg = reply
                    .iter()
                    .find_map(|w| w.strip_prefix("=message="))
                    .unwrap_or("unknown error");
                bail!("RouterOS login failed: {msg}")
            }
            other => bail!("unexpected RouterOS login response: {:?}", other),
        }
    }

    /// Run a command and return parsed key=value maps from `!re` sentences.
    pub fn run(&mut self, words: &[&str]) -> Result<Vec<HashMap<String, String>>> {
        self.sentence(words)?;
        let mut rows = Vec::new();
        loop {
            let reply = self.read_reply()?;
            match reply.first().map(|s| s.as_str()) {
                Some("!done") => break,
                Some("!re") => rows.push(parse_attrs(&reply[1..])),
                Some("!trap") => {
                    let msg = reply
                        .iter()
                        .find_map(|w| w.strip_prefix("=message="))
                        .unwrap_or("unknown error");
                    bail!("RouterOS error: {msg}")
                }
                _ => {}
            }
        }
        Ok(rows)
    }

    pub fn exec(&mut self, words: &[&str]) -> Result<()> {
        self.run(words).map(|_| ())
    }

    // ── wire ─────────────────────────────────────────────────────────────────

    fn sentence(&mut self, words: &[&str]) -> Result<()> {
        for word in words {
            write_word(&mut self.stream, word.as_bytes())?;
        }
        self.stream.write_all(&[0])?;
        self.stream.flush()?;
        Ok(())
    }

    fn read_reply(&mut self) -> Result<Vec<String>> {
        let mut words = Vec::new();
        loop {
            let len = read_length(&mut self.stream)?;
            if len == 0 {
                break;
            }
            let mut buf = vec![0u8; len];
            self.stream.read_exact(&mut buf)?;
            words.push(String::from_utf8(buf).context("RouterOS word is not UTF-8")?);
        }
        Ok(words)
    }
}

// ── RouterOS length encoding ──────────────────────────────────────────────────

fn write_word(w: &mut dyn Write, data: &[u8]) -> Result<()> {
    let len = data.len();
    if len < 0x80 {
        w.write_all(&[len as u8])?;
    } else if len < 0x4000 {
        w.write_all(&[((len >> 8) | 0x80) as u8, (len & 0xFF) as u8])?;
    } else if len < 0x200000 {
        w.write_all(&[
            ((len >> 16) | 0xC0) as u8,
            ((len >> 8) & 0xFF) as u8,
            (len & 0xFF) as u8,
        ])?;
    } else {
        w.write_all(&[
            ((len >> 24) | 0xE0) as u8,
            ((len >> 16) & 0xFF) as u8,
            ((len >> 8) & 0xFF) as u8,
            (len & 0xFF) as u8,
        ])?;
    }
    w.write_all(data)?;
    Ok(())
}

fn read_length(r: &mut dyn Read) -> Result<usize> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    let first = b[0];
    if first < 0x80 {
        return Ok(first as usize);
    }
    if first < 0xC0 {
        let mut b2 = [0u8; 1];
        r.read_exact(&mut b2)?;
        return Ok(((first as usize & 0x3F) << 8) | b2[0] as usize);
    }
    if first < 0xE0 {
        let mut b3 = [0u8; 2];
        r.read_exact(&mut b3)?;
        return Ok(((first as usize & 0x1F) << 16) | (b3[0] as usize) << 8 | b3[1] as usize);
    }
    if first < 0xF0 {
        let mut b4 = [0u8; 3];
        r.read_exact(&mut b4)?;
        return Ok(
            ((first as usize & 0x0F) << 24)
                | (b4[0] as usize) << 16
                | (b4[1] as usize) << 8
                | b4[2] as usize,
        );
    }
    bail!("unsupported RouterOS length prefix: 0x{first:02x}")
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn parse_attrs(words: &[String]) -> HashMap<String, String> {
    words
        .iter()
        .filter_map(|w| {
            let s = w.strip_prefix('=')?;
            let (k, v) = s.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect()
}
