//! The TLS 1.2 client state machine.

// --- Workspace dependencies ---
use crypto::{
    RandomSource, SecretBytes,
    agreement::x25519::{X25519, X25519PrivateKey, X25519PublicKey},
    signature::rsa_pss::{RsaPssSha256VerifyingKey, RsaPssSignature},
};
use pki::RequiredKeyUsage;
use x509::PublicKey;

// --- Internal modules ---
use super::{ClientConfig, Error};
use crate::{
    PeerChain, ServerVerification,
    handshake::{HandshakeJoiner, HandshakeMessage, Transcript},
    record::{Aes128GcmProtection, ReadState, TlsPlaintext, WriteState},
    schedule::{ConnectionEnd, MasterSecret},
    wire::{
        Alert, AlertDescription, AlertLevel, Certificate, CertificateRequest, CipherSuite,
        ClientHello, ClientKeyExchange, ContentType, EcCurveType, Extension, ExtensionType,
        Finished, HandshakeType, NamedGroup, ProtocolVersion, RecordHeader, ServerHello,
        ServerKeyExchange, SignatureScheme,
    },
};

/// The one cipher suite this client offers and accepts.
const SUITE: CipherSuite = CipherSuite::EcdheRsaWithAes128GcmSha256;

/// What the client waits for next.
enum State {
    /// The server's reply to the `ClientHello`.
    ServerHello,
    /// The server's certificate chain.
    Certificate,
    /// The server's signed ephemeral key.
    ServerKeyExchange,
    /// The end of the server's first flight, or a certificate request before it.
    ServerHelloDone,
    /// The server's switch to the new keys.
    ChangeCipherSpec,
    /// The server's `Finished`, under the new keys.
    Finished,
    /// The handshake is done; application data flows.
    Established,
    /// The connection is over: it failed, or was closed before it was established. Only
    /// buffered output and input can still be taken.
    Failed,
}

/// A sans-I/O TLS 1.2 client for one connection.
///
/// It offers exactly `TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256` with X25519 and RSA-PSS
/// signatures, requires `extended_master_secret`, and does not resume, renegotiate or present
/// a client certificate.
///
/// The driver loop: send [`transmit`](Self::transmit) and report it with
/// [`sent`](Self::sent); while [`wants_receive`](Self::wants_receive), feed what the
/// transport reads to [`receive`](Self::receive) and offer again whatever it did not take;
/// once
/// [`is_established`](Self::is_established), exchange application data with
/// [`write`](Self::write) and [`plaintext`](Self::plaintext) / [`consume`](Self::consume).
///
/// Every `Err` ends the connection, except [`Error::InvalidState`], which only refuses a
/// call that is not allowed yet or any more. Once the connection is over, only `transmit`,
/// `sent`, `plaintext`, `consume` and the accessors still work: the alert for the peer is in
/// `transmit`, and plaintext that was authenticated before the end can still be read.
pub struct Client<'a> {
    state: State,
    verification: ServerVerification<'a>,
    read: ReadState,
    write: WriteState,
    joiner: HandshakeJoiner,
    transcript: Transcript,
    outgoing: Vec<u8>,
    plaintext: Vec<u8>,
    offered: Vec<ExtensionType>,
    record_version: ProtocolVersion,
    /// The client random sent in the `ClientHello`.
    random: [u8; 32],
    server_random: [u8; 32],
    private_key: Option<X25519PrivateKey>,
    server_public: Option<X25519PublicKey>,
    certificate_requested: bool,
    master_secret: Option<MasterSecret>,
    pending_read: Option<Aes128GcmProtection>,
    peer_chain: Option<PeerChain>,
    session_id: Option<Vec<u8>>,
    close_sent: bool,
    peer_closed: bool,
    empty_records: u8,
    hello_requests: u8,
}

impl<'a> Client<'a> {
    /// Received application data above which [`receive`](Self::receive) pauses until the
    /// caller consumes some: one record's worth.
    pub const PLAINTEXT_LIMIT: usize = RecordHeader::MAX_PLAINTEXT_LEN as usize;
    /// Unsent bytes above which [`write`](Self::write) accepts no more.
    pub const TRANSMIT_LIMIT: usize = 64 * 1024;
    /// Consecutive empty application-data records tolerated before the peer is cut off.
    const MAX_EMPTY_RECORDS: u8 = 16;
    /// `HelloRequest` messages answered before the peer is cut off.
    const MAX_HELLO_REQUESTS: u8 = 4;
    /// Starts a connection: draws the client random and the ephemeral key from `random` and
    /// queues the `ClientHello`. Nothing else is ever drawn from `random`.
    ///
    /// # Errors
    ///
    /// [`Error::Crypto`] when `random` cannot supply 64 bytes, and [`Error::Codec`] when the
    /// server name is too long to encode.
    pub fn new(config: ClientConfig<'a>, random: &mut impl RandomSource) -> Result<Self, Error> {
        let mut client_random = [0; 32];
        random.fill_bytes(&mut client_random)?;
        let mut scalar = SecretBytes::new([0; 32]);
        random.fill_bytes(scalar.expose_secret_mut())?;
        let private_key = X25519PrivateKey::new(*scalar.expose_secret());

        let codec = |source| Error::Codec {
            what: "client hello",
            source,
        };
        let mut extensions = Vec::new();
        if let Some(server_name) = config.server_name {
            extensions.push(Extension::server_name(server_name).map_err(codec)?);
        }
        extensions.extend([
            Extension::supported_groups(&[NamedGroup::X25519]).map_err(codec)?,
            Extension::ec_point_formats_uncompressed(),
            Extension::signature_algorithms(&[SignatureScheme::RsaPssRsaeSha256]).map_err(codec)?,
            Extension::extended_master_secret(),
            Extension::renegotiation_info_initial(),
        ]);
        let offered = extensions.iter().map(|e| e.extension_type).collect();
        let hello = ClientHello::new(client_random, vec![SUITE], extensions);

        let mut client = Self {
            state: State::ServerHello,
            verification: config.verification,
            read: ReadState::plain(),
            write: WriteState::plain(),
            joiner: HandshakeJoiner::new(),
            transcript: Transcript::new(),
            outgoing: Vec::new(),
            plaintext: Vec::new(),
            offered,
            // A first record customarily says TLS 1.0, so that old servers answer at all.
            record_version: ProtocolVersion::TLS_1_0,
            random: client_random,
            server_random: [0; 32],
            private_key: Some(private_key),
            server_public: None,
            certificate_requested: false,
            master_secret: None,
            pending_read: None,
            peer_chain: None,
            session_id: None,
            close_sent: false,
            peer_closed: false,
            empty_records: 0,
            hello_requests: 0,
        };
        let body = hello.to_bytes().map_err(codec)?;
        client.send_handshake(HandshakeType::ClientHello, &body)?;
        Ok(client)
    }

    // --- The transport side ---

    /// Bytes waiting to be sent to the peer.
    #[must_use]
    pub fn transmit(&self) -> &[u8] {
        &self.outgoing
    }

    /// Records that the first `count` bytes of [`transmit`](Self::transmit) were sent.
    pub fn sent(&mut self, count: usize) {
        self.outgoing.drain(..count.min(self.outgoing.len()));
    }

    /// Whether [`receive`](Self::receive) would take bytes right now. `false` means the
    /// caller must consume plaintext first, or that nothing more will be read: the peer
    /// closed or the connection is over. A driver stops reading from its transport while
    /// this is `false`.
    #[must_use]
    pub fn wants_receive(&self) -> bool {
        !matches!(self.state, State::Failed) && !self.peer_closed && !self.is_backlogged()
    }

    /// Takes bytes read from the transport, processes every whole record in them, and
    /// returns how many bytes were taken. Fewer than offered were taken when the plaintext
    /// backlog reached [`Self::PLAINTEXT_LIMIT`]; the client keeps none of the rest, so
    /// offer it again after consuming. Bytes after the peer's `close_notify` are discarded.
    ///
    /// # Errors
    ///
    /// Any [`Error`] the peer's bytes cause, and [`Error::InvalidState`] after a failure.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<usize, Error> {
        self.check_alive()?;
        let mut taken = 0;
        loop {
            if let Err(error) = self.process_records() {
                return Err(self.fail(error));
            }
            if self.peer_closed {
                return Ok(bytes.len());
            }
            if self.is_backlogged() {
                return Ok(taken);
            }
            let step = self.read.receive(&bytes[taken..]);
            if step == 0 {
                return Ok(taken);
            }
            taken += step;
        }
    }

    /// Tells the client the transport reached end of input.
    ///
    /// # Errors
    ///
    /// [`Error::Truncated`] unless the peer sent `close_notify` first: without it the caller
    /// cannot know that it received everything the peer sent.
    pub fn receive_eof(&mut self) -> Result<(), Error> {
        self.check_alive()?;
        if self.peer_closed {
            return Ok(());
        }
        self.state = State::Failed;
        Err(Error::Truncated)
    }

    // --- The application side ---

    /// Whether the handshake has finished, so application data can be written.
    #[must_use]
    pub const fn is_established(&self) -> bool {
        matches!(self.state, State::Established)
    }

    /// Queues application data for the peer and returns how many bytes were accepted, which
    /// is fewer than offered (possibly zero) while more than [`Self::TRANSMIT_LIMIT`] bytes
    /// are still unsent.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidState`] before the handshake finishes, after [`close`](Self::close),
    /// or after a failure.
    pub fn write(&mut self, plaintext: &[u8]) -> Result<usize, Error> {
        if !self.is_established() || self.close_sent {
            return Err(Error::InvalidState);
        }
        let mut accepted = 0;
        for chunk in plaintext.chunks(Self::PLAINTEXT_LIMIT) {
            if self.outgoing.len() >= Self::TRANSMIT_LIMIT {
                break;
            }
            if let Err(error) = self.send_record(ContentType::ApplicationData, chunk) {
                return Err(self.fail(error));
            }
            accepted += chunk.len();
        }
        Ok(accepted)
    }

    /// Application data received and not yet consumed.
    #[must_use]
    pub fn plaintext(&self) -> &[u8] {
        &self.plaintext
    }

    /// Discards the first `count` bytes of [`plaintext`](Self::plaintext).
    pub fn consume(&mut self, count: usize) {
        self.plaintext.drain(..count.min(self.plaintext.len()));
    }

    /// Queues `close_notify`: this end will send nothing more. On an established connection
    /// receiving continues until the peer closes too, though TLS 1.2 lets a peer stop sending
    /// as soon as it sees this. Before the handshake finishes, closing abandons the
    /// connection. Calling it again does nothing.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidState`] once the connection is over.
    pub fn close(&mut self) -> Result<(), Error> {
        self.check_alive()?;
        let queued = self.queue_close_notify();
        if !self.is_established() {
            self.state = State::Failed;
        }
        queued.map_err(|error| self.fail(error))
    }

    /// Whether the peer sent `close_notify`: no more application data will arrive.
    #[must_use]
    pub const fn peer_closed(&self) -> bool {
        self.peer_closed
    }

    // --- What the handshake established ---

    /// The certificate chain the server presented, once its `Certificate` message arrived.
    #[must_use]
    pub const fn peer_chain(&self) -> Option<&PeerChain> {
        self.peer_chain.as_ref()
    }

    /// The session id the server chose, once its `ServerHello` arrived.
    #[must_use]
    pub fn session_id(&self) -> Option<&[u8]> {
        self.session_id.as_deref()
    }

    // --- Internals ---

    /// Refuses a call on a failed connection.
    const fn check_alive(&self) -> Result<(), Error> {
        match self.state {
            State::Failed => Err(Error::InvalidState),
            _ => Ok(()),
        }
    }

    /// Whether enough plaintext is waiting that no more records should be opened.
    fn is_backlogged(&self) -> bool {
        self.plaintext.len() >= Self::PLAINTEXT_LIMIT
    }

    /// Marks the connection failed and queues the alert that tells the peer why, unless
    /// this end has already said it will send nothing more.
    fn fail(&mut self, error: Error) -> Error {
        if let (Some(description), false) = (error.alert(), self.close_sent) {
            let alert = Alert::new(AlertLevel::Fatal, description);
            // Best effort: if the alert itself cannot be sealed there is nothing left to say.
            self.send_alert(alert).ok();
        }
        self.close_sent = true;
        self.state = State::Failed;
        error
    }

    /// Queues `close_notify` once. Unlike [`close`](Self::close) it leaves failing to the
    /// caller, so an error on the receive path is handled in one place.
    fn queue_close_notify(&mut self) -> Result<(), Error> {
        if self.close_sent {
            return Ok(());
        }
        self.close_sent = true;
        self.send_alert(Alert::new(
            AlertLevel::Warning,
            AlertDescription::CloseNotify,
        ))
    }

    /// Queues one record under the write state in force.
    fn send_record(&mut self, content_type: ContentType, plaintext: &[u8]) -> Result<(), Error> {
        let record = self
            .write
            .seal(content_type, self.record_version, plaintext)?;
        self.outgoing.extend(record);
        Ok(())
    }

    /// Queues an alert.
    fn send_alert(&mut self, alert: Alert) -> Result<(), Error> {
        let body = [u8::from(alert.level), u8::from(alert.description)];
        self.send_record(ContentType::Alert, &body)
    }

    /// Queues a handshake message and adds it to the transcript.
    fn send_handshake(&mut self, msg_type: HandshakeType, body: &[u8]) -> Result<(), Error> {
        let message = HandshakeMessage::new(msg_type, body);
        self.transcript.add(&message);
        self.send_record(ContentType::Handshake, message.bytes())
    }

    /// Opens and handles every whole record buffered, stopping at a backlog or a close.
    fn process_records(&mut self) -> Result<(), Error> {
        while !self.is_backlogged() && !self.peer_closed {
            let Some(record) = self.read.next_record()? else {
                break;
            };
            self.handle_record(record)?;
        }
        Ok(())
    }

    /// Dispatches one record by its content type, enforcing what may appear when.
    fn handle_record(&mut self, record: TlsPlaintext) -> Result<(), Error> {
        let version_ok = match self.state {
            State::ServerHello => record.version.major == ProtocolVersion::TLS_1_2.major,
            _ => record.version == ProtocolVersion::TLS_1_2,
        };
        if !version_ok {
            return Err(Error::ProtocolVersion);
        }
        if record.content_type != ContentType::Handshake && !self.joiner.is_empty() {
            return Err(Error::UnexpectedMessage(
                "record interleaved with a handshake message",
            ));
        }
        match record.content_type {
            ContentType::Handshake => self.handle_handshake_record(&record.fragment),
            ContentType::ChangeCipherSpec => self.handle_change_cipher_spec(&record.fragment),
            ContentType::Alert => self.handle_alert(&record.fragment),
            ContentType::ApplicationData => self.handle_application_data(record.fragment),
            ContentType::Other(_) => Err(Error::UnexpectedMessage("unknown record type")),
        }
    }

    /// Joins a handshake record's fragment into messages and handles each.
    fn handle_handshake_record(&mut self, fragment: &[u8]) -> Result<(), Error> {
        if fragment.is_empty() {
            return Err(Error::UnexpectedMessage("empty handshake record"));
        }
        self.joiner.push(fragment);
        while let Some(message) = self.joiner.next_message()? {
            self.handle_message(&message)?;
        }
        Ok(())
    }

    /// Switches to the server's new keys, which is only allowed right after our `Finished`.
    fn handle_change_cipher_spec(&mut self, fragment: &[u8]) -> Result<(), Error> {
        if !matches!(self.state, State::ChangeCipherSpec) {
            return Err(Error::UnexpectedMessage("ChangeCipherSpec"));
        }
        if fragment != [1] {
            return Err(Error::IllegalParameter("ChangeCipherSpec body"));
        }
        let protection = self.pending_read.take().ok_or(Error::InvalidState)?;
        self.read.protect(protection);
        self.state = State::Finished;
        Ok(())
    }

    /// Handles an alert: `close_notify` on an established connection ends the peer's data,
    /// anything else ends the connection.
    fn handle_alert(&mut self, fragment: &[u8]) -> Result<(), Error> {
        let alert = Alert::decode_exact(fragment).map_err(|source| Error::Codec {
            what: "alert",
            source,
        })?;
        if alert.description != AlertDescription::CloseNotify || !self.is_established() {
            return Err(Error::PeerAlert(alert));
        }
        self.peer_closed = true;
        // RFC 5246 §7.2.1: answer a close_notify with our own.
        self.queue_close_notify()
    }

    /// Queues received application data for the caller.
    fn handle_application_data(&mut self, fragment: Vec<u8>) -> Result<(), Error> {
        if !self.is_established() {
            return Err(Error::UnexpectedMessage(
                "application data during the handshake",
            ));
        }
        if fragment.is_empty() {
            self.empty_records += 1;
            if self.empty_records > Self::MAX_EMPTY_RECORDS {
                return Err(Error::UnexpectedMessage("too many empty records"));
            }
            return Ok(());
        }
        self.empty_records = 0;
        self.plaintext.extend(fragment);
        Ok(())
    }

    /// Handles one handshake message, which must be the one the state expects.
    fn handle_message(&mut self, message: &HandshakeMessage) -> Result<(), Error> {
        match (&self.state, message.msg_type()) {
            (State::ServerHello, HandshakeType::ServerHello) => self.on_server_hello(message),
            (State::Certificate, HandshakeType::Certificate) => self.on_certificate(message),
            (State::ServerKeyExchange, HandshakeType::ServerKeyExchange) => {
                self.on_server_key_exchange(message)
            }
            (State::ServerHelloDone, HandshakeType::CertificateRequest)
                if !self.certificate_requested =>
            {
                self.on_certificate_request(message)
            }
            (State::ServerHelloDone, HandshakeType::ServerHelloDone) => {
                self.on_server_hello_done(message)
            }
            (State::Finished, HandshakeType::Finished) => self.on_finished(message),
            (State::Established, HandshakeType::HelloRequest) => self.on_hello_request(message),
            _ => Err(Error::UnexpectedMessage("handshake message out of order")),
        }
    }

    /// Checks that the server chose what was offered, and nothing else.
    fn on_server_hello(&mut self, message: &HandshakeMessage) -> Result<(), Error> {
        let hello = ServerHello::decode_exact(message.body()).map_err(|source| Error::Codec {
            what: "server hello",
            source,
        })?;
        if hello.server_version != ProtocolVersion::TLS_1_2 {
            return Err(Error::ProtocolVersion);
        }
        if hello.cipher_suite != SUITE {
            return Err(Error::IllegalParameter("cipher suite"));
        }
        if hello.compression_method != ClientHello::COMPRESSION_NULL {
            return Err(Error::IllegalParameter("compression method"));
        }
        if hello.session_id.len() > 32 {
            return Err(Error::IllegalParameter("session id length"));
        }
        let mut seen = Vec::new();
        for extension in hello.extensions.iter().flatten() {
            let extension_type = extension.extension_type;
            if !self.offered.contains(&extension_type) || seen.contains(&extension_type) {
                return Err(Error::UnsupportedExtension(extension_type));
            }
            seen.push(extension_type);
        }
        let renegotiation = hello.extension(ExtensionType::RenegotiationInfo);
        if renegotiation.is_some_and(|info| info.data != [0]) {
            return Err(Error::HandshakeFailure("renegotiation info is not empty"));
        }
        // RFC 7627 §5.2: without it the master secret is not bound to this handshake.
        if hello
            .extension(ExtensionType::ExtendedMasterSecret)
            .is_none()
        {
            return Err(Error::HandshakeFailure(
                "server does not support extended master secret",
            ));
        }
        self.server_random = hello.random;
        self.session_id = Some(hello.session_id);
        self.record_version = ProtocolVersion::TLS_1_2;
        self.transcript.add(message);
        self.state = State::Certificate;
        Ok(())
    }

    /// Takes the server's chain and applies the caller's verification policy to it.
    fn on_certificate(&mut self, message: &HandshakeMessage) -> Result<(), Error> {
        let certificate =
            Certificate::decode_exact(message.body()).map_err(|source| Error::Codec {
                what: "certificate",
                source,
            })?;
        let chain = PeerChain::from(certificate);
        // An ECDHE suite uses the certificate's key to sign (RFC 5246 §7.4.2).
        self.verification
            .verify(&chain, RequiredKeyUsage::DigitalSignature)?;
        self.peer_chain = Some(chain);
        self.transcript.add(message);
        self.state = State::ServerKeyExchange;
        Ok(())
    }

    /// Checks the server's ephemeral key and its signature by the certificate's key. This
    /// runs under every verification policy: an unverified chain still has to be the one
    /// whose key the server holds.
    fn on_server_key_exchange(&mut self, message: &HandshakeMessage) -> Result<(), Error> {
        let body = message.body();
        let exchange = ServerKeyExchange::decode_exact(body).map_err(|source| Error::Codec {
            what: "server key exchange",
            source,
        })?;
        let params = &exchange.params;
        if params.curve_type != EcCurveType::NamedCurve || params.named_group != NamedGroup::X25519
        {
            return Err(Error::IllegalParameter("key exchange group"));
        }
        let public: [u8; 32] = params
            .public
            .as_slice()
            .try_into()
            .map_err(|_| Error::IllegalParameter("key exchange public key length"))?;
        if exchange.algorithm != SignatureScheme::RsaPssRsaeSha256 {
            return Err(Error::IllegalParameter("signature algorithm"));
        }

        let chain = self.peer_chain.as_ref().ok_or(Error::InvalidState)?;
        let PublicKey::Rsa { modulus, exponent } = chain.leaf_public_key()? else {
            return Err(Error::HandshakeFailure("server certificate key is not RSA"));
        };
        let unacceptable = Error::HandshakeFailure("server RSA key is not acceptable");
        // This also refuses a modulus under 2048 bits.
        let key = RsaPssSha256VerifyingKey::from_components(modulus, exponent)
            .map_err(|_| unacceptable)?;
        // The signature covers the parameters exactly as received: curve type, group, and
        // the key behind its length byte.
        let signed_params = &body[..4 + public.len()];
        let signed = [&self.random, &self.server_random, signed_params].concat();
        key.verify_sha256(signed, &RsaPssSignature::from_bytes(exchange.signature))
            .map_err(|_| Error::DecryptError("server key exchange signature"))?;

        self.server_public = Some(X25519PublicKey::new(public));
        self.transcript.add(message);
        self.state = State::ServerHelloDone;
        Ok(())
    }

    /// Notes that the server asked for a client certificate. What it would accept is not
    /// looked at, because there is none to offer; the request only has to be well formed.
    fn on_certificate_request(&mut self, message: &HandshakeMessage) -> Result<(), Error> {
        CertificateRequest::decode_exact(message.body()).map_err(|source| Error::Codec {
            what: "certificate request",
            source,
        })?;
        self.certificate_requested = true;
        self.transcript.add(message);
        Ok(())
    }

    /// Sends the client's whole second flight and derives the keys.
    fn on_server_hello_done(&mut self, message: &HandshakeMessage) -> Result<(), Error> {
        if !message.body().is_empty() {
            return Err(Error::IllegalParameter("ServerHelloDone body"));
        }
        self.transcript.add(message);
        let codec = |source| Error::Codec {
            what: "client key exchange",
            source,
        };
        if self.certificate_requested {
            // No client certificate is configured, which an empty list says.
            let empty = Certificate {
                certificate_list: Vec::new(),
            };
            self.send_handshake(
                HandshakeType::Certificate,
                &empty.to_bytes().map_err(codec)?,
            )?;
        }
        let private_key = self.private_key.take().ok_or(Error::InvalidState)?;
        let server_public = self.server_public.take().ok_or(Error::InvalidState)?;
        let exchange = ClientKeyExchange {
            public: X25519::public_key(&private_key).as_bytes().to_vec(),
        };
        self.send_handshake(
            HandshakeType::ClientKeyExchange,
            &exchange.to_bytes().map_err(codec)?,
        )?;

        // An all-zero shared secret means the server sent a low-order point.
        let shared = X25519::agree(&private_key, &server_public)
            .map_err(|_| Error::IllegalParameter("key exchange public key"))?;
        let master_secret =
            MasterSecret::derive_extended(shared.expose_secret(), &self.transcript.hash())?;
        let keys = master_secret.aes128_gcm_keys(&self.random, &self.server_random)?;

        self.send_record(ContentType::ChangeCipherSpec, &[1])?;
        self.write.protect(keys.client_write);
        self.pending_read = Some(keys.server_write);
        let verify_data =
            master_secret.finished_verify_data(ConnectionEnd::Client, &self.transcript.hash())?;
        self.send_handshake(HandshakeType::Finished, &verify_data)?;

        self.master_secret = Some(master_secret);
        self.state = State::ChangeCipherSpec;
        Ok(())
    }

    /// Checks the server's `Finished`, which proves it derived the same keys from the same
    /// handshake.
    fn on_finished(&mut self, message: &HandshakeMessage) -> Result<(), Error> {
        let finished = Finished::decode_exact(message.body()).map_err(|source| Error::Codec {
            what: "finished",
            source,
        })?;
        if finished.verify_data.len() != Finished::VERIFY_DATA_LEN {
            return Err(Error::IllegalParameter("Finished length"));
        }
        let master_secret = self.master_secret.take().ok_or(Error::InvalidState)?;
        let expected = SecretBytes::new(
            master_secret.finished_verify_data(ConnectionEnd::Server, &self.transcript.hash())?,
        );
        if !expected.matches(&finished.verify_data) {
            return Err(Error::DecryptError("server Finished"));
        }
        self.state = State::Established;
        Ok(())
    }

    /// Refuses a renegotiation request with a warning, a bounded number of times.
    fn on_hello_request(&mut self, message: &HandshakeMessage) -> Result<(), Error> {
        if !message.body().is_empty() {
            return Err(Error::IllegalParameter("HelloRequest body"));
        }
        self.hello_requests += 1;
        if self.hello_requests > Self::MAX_HELLO_REQUESTS {
            return Err(Error::UnexpectedMessage("too many hello requests"));
        }
        if self.close_sent {
            return Ok(());
        }
        let alert = Alert::new(AlertLevel::Warning, AlertDescription::NoRenegotiation);
        self.send_alert(alert)
    }
}
