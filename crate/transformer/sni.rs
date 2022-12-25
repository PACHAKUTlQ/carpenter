use std::io;
use std::io::{Read, Write};
use rustls::{ServerConnection, ClientConnection, ServerConfig, ClientConfig};
use super::streambuffer::StreamBuffer;
use super::certstorage::get_cert_data_by_hostname;
use super::{TransformerResult, TransformerPortState, Transformer};
use crate::common::Hostname;

const SINGLE_BRUST_SIZE_LIMIT: usize = 512 * 1024; // = 512 KB

pub struct SniRewriterTransformer {
    local_tls: ServerConnection,
    remote_tls: ClientConnection,
    transmit_text: StreamBuffer,
    receive_text: StreamBuffer,
}

impl SniRewriterTransformer {
    pub fn new(_host_name_str: &str, new_sni: Option<Hostname>, raw_sni: Hostname) -> io::Result<Self> {
        let host_name = raw_sni.clone();

        let (local_tls_cert_data, local_tls_pkey_data) =
            get_cert_data_by_hostname(&host_name).unwrap(); // TODO

        let local_tls_conf = std::sync::Arc::new(
            ServerConfig::builder()
                .with_safe_defaults()
                .with_no_client_auth()
                .with_single_cert(local_tls_cert_data, local_tls_pkey_data)
                .expect("bad local_tls_conf")
        );
        let mut root_store = rustls::RootCertStore::empty();
        root_store.add_server_trust_anchors(
            webpki_roots::TLS_SERVER_ROOTS
                .0
                .iter()
                .map(|ta| {
                    rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
                        ta.subject,
                        ta.spki,
                        ta.name_constraints,
                    )
                })
        );
        let mut remote_tls_conf =
            ClientConfig::builder()
                .with_safe_defaults()
                .with_root_certificates(root_store)
                .with_no_client_auth();
        if new_sni.is_none() {
            remote_tls_conf.enable_sni = false;
        }
        let remote_tls_conf = std::sync::Arc::new(remote_tls_conf);

        let mut local_tls =  ServerConnection::new(local_tls_conf).unwrap();
        local_tls.set_buffer_limit(Some(SINGLE_BRUST_SIZE_LIMIT * 2));
        let mut remote_tls = ClientConnection::new(remote_tls_conf,
            convert_hostname_to_rustls_server_name(new_sni.unwrap_or(raw_sni).clone()),
        ).unwrap();
        remote_tls.set_buffer_limit(Some(SINGLE_BRUST_SIZE_LIMIT * 2));

        Ok(Self {
            local_tls, 
            remote_tls,
            transmit_text: StreamBuffer::new(),
            receive_text: StreamBuffer::new(),
        })
    }
}


impl Transformer for SniRewriterTransformer {
    /* transmit tube */

    fn transmit_writable(&self) -> TransformerPortState {
        if self.local_tls.wants_read() {
            TransformerPortState::Open(-1)
        } else {
            TransformerPortState::Open(0)
        }
    }

    fn transmit_write(&mut self, mut buf: &[u8]) -> TransformerResult {
        if !self.local_tls.wants_read() {
            return TransformerResult::Ok(0)
        }

        let read_tls_result = self.local_tls.read_tls(&mut buf);
        if read_tls_result.is_err() {
            return TransformerResult::IoError(read_tls_result.unwrap_err());
        }
        let read_tls_size = read_tls_result.unwrap();
        if read_tls_size == 0 {
            return TransformerResult::Ok(read_tls_size);
        }

        let r = self.local_tls.process_new_packets();
        if r.is_err() {
            return TransformerResult::ProtocolError(r.unwrap_err());
        }
        let state = r.unwrap();
        let plaintext_size = state.plaintext_bytes_to_read();
        let peer_closed = state.peer_has_closed();

        // read out plain texts, write them into its pair tls or the buffer. 
        let mut text_buffer = Vec::new();
        text_buffer.resize(plaintext_size, 0);
        let text_read_size = self.local_tls.reader().read(&mut text_buffer).unwrap();
        assert_eq!(text_read_size, plaintext_size); 
        let text_write_size = self.remote_tls.writer().write(&text_buffer).unwrap();
        if text_write_size < text_read_size {
            self.transmit_text.write(&text_buffer[text_write_size..]).unwrap();
        }

        if peer_closed {
            self.remote_tls.send_close_notify();
        }

        return TransformerResult::Ok(read_tls_size);
    }

    fn transmit_readable(&self) -> TransformerPortState {
        if self.remote_tls.wants_write() || self.transmit_text.readable_size() > 0 {
            TransformerPortState::Open(-1)
        } else {
            TransformerPortState::Open(0)
        }
    }

    fn transmit_read(&mut self, mut buf: &mut [u8]) -> TransformerResult {
        if !self.remote_tls.wants_write() {
            // try to feed some plain text
            let plaintext_size = std::cmp::min(self.transmit_text.readable_size(), SINGLE_BRUST_SIZE_LIMIT);
            let mut plaintext_buf = Vec::new();
            plaintext_buf.resize(plaintext_size, 0);
            self.transmit_text.read(&mut plaintext_buf).unwrap();
            self.remote_tls.writer().write(&plaintext_buf).unwrap();

            // then check again
            if !self.remote_tls.wants_write() {
                return TransformerResult::Ok(0);
            }
        }

        let write_tls_result = self.remote_tls.write_tls(&mut buf);
        if write_tls_result.is_err() {
            return TransformerResult::IoError(write_tls_result.unwrap_err());
        }
        TransformerResult::Ok(write_tls_result.unwrap())
    }

    /* receive tube */

    fn receive_writable(&self) -> TransformerPortState {
        if self.remote_tls.wants_read() {
            TransformerPortState::Open(-1)
        } else {
            TransformerPortState::Open(0)
        }
    }

    fn receive_write(&mut self, mut buf: &[u8]) -> TransformerResult {
        if !self.remote_tls.wants_read() {
            return TransformerResult::Ok(0)
        }

        let read_tls_result = self.remote_tls.read_tls(&mut buf);
        if read_tls_result.is_err() {
            return TransformerResult::IoError(read_tls_result.unwrap_err());
        }
        let read_tls_size = read_tls_result.unwrap();
        if read_tls_size == 0 {
            return TransformerResult::Ok(read_tls_size);
        }

        let r = self.remote_tls.process_new_packets();
        if r.is_err() {
            return TransformerResult::ProtocolError(r.unwrap_err());
        }
        let state = r.unwrap();
        let plaintext_size = state.plaintext_bytes_to_read();
        let peer_closed = state.peer_has_closed();

        // read out plain texts, write them into its pair tls or the buffer. 
        let mut text_buffer = Vec::new();
        text_buffer.resize(plaintext_size, 0);
        let text_read_size = self.remote_tls.reader().read(&mut text_buffer).unwrap();
        assert_eq!(text_read_size, plaintext_size); 
        let text_write_size = self.local_tls.writer().write(&text_buffer).unwrap();
        if text_write_size < text_read_size {
            self.receive_text.write(&text_buffer[text_write_size..]).unwrap();
        }

        if peer_closed {
            self.remote_tls.send_close_notify();
        }

        return TransformerResult::Ok(read_tls_size);
    }

    fn receive_readable(&self) -> TransformerPortState {
        if self.local_tls.wants_write() || self.receive_text.readable_size() > 0 {
            TransformerPortState::Open(-1)
        } else {
            TransformerPortState::Open(0)
        }
    }

    fn receive_read(&mut self, mut buf: &mut [u8]) -> TransformerResult {
        if !self.local_tls.wants_write() {
            // try to feed some plain text
            let plaintext_size = std::cmp::min(self.receive_text.readable_size(), SINGLE_BRUST_SIZE_LIMIT);
            let mut plaintext_buf = Vec::new();
            plaintext_buf.resize(plaintext_size, 0);
            self.receive_text.read(&mut plaintext_buf).unwrap();
            self.local_tls.writer().write(&plaintext_buf).unwrap();

            // then check again
            if !self.local_tls.wants_write() {
                return TransformerResult::Ok(0);
            }
        }

        let write_tls_result = self.local_tls.write_tls(&mut buf);
        if write_tls_result.is_err() {
            return TransformerResult::IoError(write_tls_result.unwrap_err());
        }
        TransformerResult::Ok(write_tls_result.unwrap())
    }
}



fn convert_hostname_to_rustls_server_name(h: Hostname) -> rustls::client::ServerName {
    use rustls::client::ServerName;
    match h {
        Hostname::Addr4(v) => ServerName::IpAddress(std::net::IpAddr::V4(v)),
        Hostname::Addr6(v) => ServerName::IpAddress(std::net::IpAddr::V6(v)),
        Hostname::Domain(v) => ServerName::try_from(v.as_str()).unwrap(),
    }
}

