//! The state a gateway hands a client as it logs in.

// --- Standard library ---
use std::fmt;
use std::str;

// --- Workspace dependencies ---
use cookie::Jar;
use crypto::RandomSource;
use sangfor_atrust_sig::{NonceGenerator, SignKey, identifiers};

// --- Internal modules ---
use super::{Device, Error};
use crate::api::{self, AuthConfig, Call, Context, Gateway, Portal, Reply};

/// The name of the cookie that identifies a session (spec §2.3).
const SID: &[u8] = b"sid";

/// One client's session with one gateway: its cookies, and what the latest `authConfig`
/// answer established (spec §5.1).
///
/// A session starts empty and is filled by [`login`](Self::login). It outlives the login:
/// later control-plane requests need its cookies and its CSRF token.
pub struct Session {
    /// The gateway.
    gateway: Gateway,
    /// The device this client reports.
    device: Device,
    /// The session's cookies. One gateway is one host, so one jar.
    pub(super) jar: Jar,
    /// The latest CSRF token. Empty before the first `authConfig` answer.
    csrf_token: String,
    /// The sign key of the latest `authConfig` answer.
    sign_key: Option<SignKey>,
    /// The modulus of the key secrets are encrypted under, as received.
    pub(super) pub_key: String,
    /// That key's exponent, as received.
    pub(super) pub_key_exp: String,
    /// The value mixed into the next encrypted secret. A password answer may carry a fresher
    /// one; it is not kept until a later step encrypts with it.
    pub(super) anti_replay_rand: String,
    /// The generator of anti-MITM nonces, started at the first one.
    nonces: Option<NonceGenerator>,
}

impl Session {
    /// A session with `gateway` that has not logged in, for a client reporting `device`.
    #[must_use]
    pub fn new(gateway: Gateway, device: Device) -> Self {
        Self {
            gateway,
            device,
            jar: Jar::new(),
            csrf_token: String::new(),
            sign_key: None,
            pub_key: String::new(),
            pub_key_exp: String::new(),
            anti_replay_rand: String::new(),
            nonces: None,
        }
    }

    /// Carry the values used before authConfig into an isolated login attempt.
    pub(super) fn strict_attempt(&mut self) -> Self {
        let mut attempt = Self::new(self.gateway.clone(), self.device.clone());
        attempt.jar = self.jar.clone();
        attempt.csrf_token.clone_from(&self.csrf_token);
        attempt.nonces = self.nonces.take();
        attempt
    }

    /// Commit authenticated state on success and never reuse an issued nonce on retry.
    pub(super) fn finish_strict_attempt(&mut self, attempt: Self, success: bool) {
        if success {
            *self = attempt;
        } else {
            self.nonces = attempt.nonces;
        }
    }

    /// The gateway.
    #[must_use]
    pub const fn gateway(&self) -> &Gateway {
        &self.gateway
    }

    /// The device this client reports.
    #[must_use]
    pub const fn device(&self) -> &Device {
        &self.device
    }

    /// The session's cookies.
    #[must_use]
    pub const fn cookies(&self) -> &Jar {
        &self.jar
    }

    /// The session's cookies, to restore stored ones into before a login (spec §2.3). A
    /// gateway that still knows them answers that the session is logged in, and the login
    /// then asks for no password.
    pub const fn cookies_mut(&mut self) -> &mut Jar {
        &mut self.jar
    }

    /// The session identifier: the value of the `sid` cookie the jar would send to the
    /// gateway's root at the time `now`, in seconds since the Unix epoch (spec §5.2). `None`
    /// when there is none or it is not text. A `sid` scoped to a narrower path is not it.
    #[must_use]
    pub fn sid(&self, now: i64) -> Option<&str> {
        let sid = self.jar.cookies().iter().find(|cookie| {
            cookie.name == SID && !cookie.is_expired(now) && cookie.matches_path(b"/")
        })?;
        str::from_utf8(&sid.value).ok()
    }

    /// The sign key of the latest `authConfig` answer.
    pub(super) fn sign_key(&self) -> Result<&SignKey, Error> {
        self.sign_key
            .as_ref()
            .ok_or(Error::Api(api::Error::NoSignKey))
    }

    /// The next anti-MITM nonce. The generator is seeded with the time of its first use.
    pub(super) fn next_nonce(&mut self, now: i64) -> String {
        let seed = u64::try_from(now).unwrap_or(0);
        self.nonces
            .get_or_insert_with(|| NonceGenerator::new(seed))
            .next_nonce()
    }

    /// Send `call` in this session and take the cookies its answer sets.
    pub(super) fn send(
        &mut self,
        portal: &mut impl Portal,
        call: &Call,
        random: &mut impl RandomSource,
        now: i64,
    ) -> Result<Reply, Error> {
        let trace_id = identifiers::trace_id(random)?;
        let path = call.path().as_bytes();
        let cookie = self.jar.header(path, now);
        let request = call.request(&Context {
            gateway: &self.gateway,
            csrf_token: &self.csrf_token,
            trace_id: &trace_id,
            sign_key: self.sign_key.as_ref(),
            cookie: cookie.as_deref(),
        })?;
        let reply = portal.exchange(&request).map_err(Error::Transport)?;
        for set_cookie in reply.response.head.headers.get_all("Set-Cookie") {
            self.jar.store(set_cookie, path, now);
        }
        Ok(reply)
    }

    /// Take what an `authConfig` answer establishes: the CSRF token, the key secrets are
    /// encrypted under, the anti-replay value and the sign key. Each replaces the value held
    /// before, empty or not (spec §5.2), and a sign key is derived even when the answer has
    /// no anti-MITM object (spec §3.3).
    pub(super) fn adopt(&mut self, config: &AuthConfig) -> Result<(), Error> {
        self.csrf_token.clone_from(&config.csrf_token);
        self.pub_key.clone_from(&config.pub_key);
        self.pub_key_exp.clone_from(&config.pub_key_exp);
        self.anti_replay_rand.clone_from(&config.anti_replay_rand);
        let (modulus, exponent, challenge) =
            config.anti_mitm.as_ref().map_or(("", "", ""), |data| {
                (
                    data.device_pub_key_mod.as_str(),
                    data.device_pub_key_exp.as_str(),
                    data.challenge.as_str(),
                )
            });
        self.sign_key = Some(SignKey::derive(modulus, exponent, challenge)?);
        Ok(())
    }
}

impl fmt::Debug for Session {
    /// Shows whose session it is, and none of what it holds: the cookies carry the session
    /// identifier, which is all a tunnel needs to act as the user.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Session")
            .field("gateway", &self.gateway)
            .field("device", &self.device.id)
            .finish_non_exhaustive()
    }
}
