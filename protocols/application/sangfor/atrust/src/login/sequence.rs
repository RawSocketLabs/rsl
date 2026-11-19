//! The login sequence.

// --- Workspace dependencies ---
use crypto::RandomSource;
use http1::Response;
use sangfor_atrust_sig::{
    SignKey, canonical_string, chain_matches, challenge_passes, encrypt_secret,
};

// --- Internal modules ---
use super::{AntiMitm, Check, Credentials, Error, Login, Options, Session, Step};
use crate::api::{
    self, AntiMitmData, AntiMitmReply, AuthConfig, Call, Envelope, Manifest, NextStep, OnlineInfo,
    Portal, PswReply,
};
use crate::policy::Policy;

/// The login method this sequence uses.
const PASSWORD: &str = "auth/psw";
/// The step a secondary chain starts with when the primary answer names none.
const AUTH_CHECK: &str = "auth/authCheck";
/// The codes with which a second-stage answer says the check failed (spec §3.6).
const SECOND_STAGE_REFUSALS: [i64; 2] = [10_000_004, 10_000_008];

impl Session {
    /// Log in with a password (spec §5.2) and fetch the policy.
    ///
    /// `now` is the time in seconds since the Unix epoch; cookies expire against it.
    /// `random` supplies trace ids and the padding of the encrypted password.
    ///
    /// When the gateway already knows the session's cookies, no password is sent.
    ///
    /// The portal controls TLS verification. The protocol reports its separate anti-MITM
    /// checks via [`Login::anti_mitm`]. By default the password is sent even when those
    /// checks fail; see [`Options::strict_anti_mitm`].
    ///
    /// # Errors
    ///
    /// [`Error`] says which step failed. A failed strict login discards cookies and key
    /// material received during the attempt. Advisory mode retains partial session state.
    pub fn login(
        &mut self,
        portal: &mut impl Portal,
        credentials: &Credentials<'_>,
        options: Options,
        random: &mut impl RandomSource,
        now: i64,
    ) -> Result<Login, Error> {
        if !options.strict_anti_mitm {
            return self.login_attempt(portal, credentials, options, random, now);
        }
        let mut attempt = self.strict_attempt();
        let result = attempt.login_attempt(portal, credentials, options, random, now);
        self.finish_strict_attempt(attempt, result.is_ok());
        result
    }

    /// Perform the exchanges against the session state for this attempt.
    fn login_attempt(
        &mut self,
        portal: &mut impl Portal,
        credentials: &Credentials<'_>,
        options: Options,
        random: &mut impl RandomSource,
        now: i64,
    ) -> Result<Login, Error> {
        let manifest =
            Manifest::parse(&self.send(portal, &Call::manifest(), random, now)?.response)?;

        let reply = self.send(portal, &Call::auth_config(), random, now)?;
        let config = AuthConfig::parse(&reply.response)?;
        self.adopt(&config)?;
        let anti_mitm = match &config.anti_mitm {
            Some(data) => self.check_anti_mitm(portal, data, &reply.peer_chain, random, now)?,
            None => AntiMitm::Absent,
        };
        if let (AntiMitm::Failed(check), true) = (anti_mitm, options.strict_anti_mitm) {
            return Err(Error::AntiMitm(check));
        }

        if !config.is_login {
            self.authenticate(portal, &config, credentials, options, random, now)?;
        }

        let info = self.send(portal, &Call::online_info(), random, now)?;
        let user_name = OnlineInfo::parse(&info.response)?.username;

        // The specification's reference does not look at this answer's code; an answer that
        // refuses is no policy, so this does.
        let resource = self.send(portal, &Call::client_resource(), random, now)?;
        Envelope::parse(&resource.response.body)?.accepted()?;
        let policy = Policy::parse(&resource.response.body, self.gateway().host())?;

        // Read last: any answer above may have replaced it.
        let sid = self.sid(now).filter(|sid| !sid.is_empty());
        Ok(Login {
            sid: sid.ok_or(Error::NoSession)?.to_owned(),
            user_name,
            policy,
            zero_rtt: manifest.zero_rtt,
            anti_mitm,
        })
    }

    /// Run the anti-MITM checks on an `authConfig` answer, in order, stopping at the first
    /// that fails (spec §5.3). `peer_chain` is the certificate chain of the connection the
    /// answer came over.
    fn check_anti_mitm(
        &mut self,
        portal: &mut impl Portal,
        data: &AntiMitmData,
        peer_chain: &[Vec<u8>],
        random: &mut impl RandomSource,
        now: i64,
    ) -> Result<AntiMitm, Error> {
        let challenge_ok = challenge_passes(
            &data.device_pub_key_mod,
            &data.device_pub_key_exp,
            &data.challenge,
            &data.encrypted_challenge,
        )?;
        if !challenge_ok {
            return Ok(AntiMitm::Failed(Check::Challenge));
        }
        // An object too odd to canonicalise is one whose signature cannot hold.
        let signed = match canonical_string(&data.raw) {
            Ok(canonical) => self
                .sign_key()?
                .verify(canonical.as_bytes(), &data.mitm_sig)?,
            Err(_) => false,
        };
        if !signed {
            return Ok(AntiMitm::Failed(Check::Signature));
        }
        if data.enable != 1 {
            return Ok(AntiMitm::Passed);
        }
        if !chain_matches(peer_chain, &data.rsa_cert, &data.sm2_enc_cert) {
            return Ok(AntiMitm::Failed(Check::Certificate));
        }
        if data.anti_mitm_request {
            return Ok(AntiMitm::Passed);
        }

        let nonce = self.next_nonce(now);
        let call = Call::anti_mitm(&nonce, &data.ticket, &self.device().id);
        let passed = match self.send(portal, &call, random, now) {
            Ok(reply) => second_stage_passes(self.sign_key()?, &reply.response, &nonce)?,
            // The check is advisory, so an exchange that fails is a check that fails.
            Err(Error::Transport(_)) => false,
            Err(error) => return Err(error),
        };
        Ok(if passed {
            AntiMitm::Passed
        } else {
            AntiMitm::Failed(Check::SecondStage)
        })
    }

    /// Log in with the password, report the environment, and follow the secondary chain
    /// (spec §5.2, §5.4).
    fn authenticate(
        &mut self,
        portal: &mut impl Portal,
        config: &AuthConfig,
        credentials: &Credentials<'_>,
        options: Options,
        random: &mut impl RandomSource,
        now: i64,
    ) -> Result<(), Error> {
        if config.server(PASSWORD, credentials.domain).is_none() {
            return Err(Error::NoLoginMethod {
                domain: credentials.domain.to_owned(),
            });
        }
        // A key that cannot be used fails here, before anything is sent.
        let encrypted = encrypt_secret(
            &self.pub_key,
            &self.pub_key_exp,
            credentials.password,
            &self.anti_replay_rand,
            random,
        )?;
        let call = Call::psw(
            &credentials.qualified_username(),
            &encrypted,
            &self.device().id,
        );
        let reply = PswReply::parse(&self.send(portal, &call, random, now)?.response)?;
        if reply.wants_captcha {
            return Err(Error::CaptchaRequired);
        }
        if reply.code != 0 {
            return Err(Error::Api(api::Error::Refused {
                code: reply.code,
                message: reply.message,
            }));
        }
        if reply.ticket.is_empty() {
            return Err(Error::NoTicket);
        }
        let device = self.device().clone();
        let report = Call::report_env(&device.id, &device.mac_addresses, &reply.ticket);
        let reported = self.send(portal, &report, random, now)?;
        Envelope::parse(&reported.response.body)?.accepted()?;

        self.follow_chain(portal, reply.next_service, options, random, now)
    }

    /// Follow the secondary chain from `service` until the gateway names no further step
    /// (spec §5.5). The primary answer's service is taken literally, and an empty one means
    /// the default first step.
    fn follow_chain(
        &mut self,
        portal: &mut impl Portal,
        service: String,
        options: Options,
        random: &mut impl RandomSource,
        now: i64,
    ) -> Result<(), Error> {
        let mut step = Step {
            service: if service.is_empty() {
                AUTH_CHECK.to_owned()
            } else {
                service
            },
            auth_id: String::new(),
        };
        for _ in 0..options.max_steps {
            if step.service != AUTH_CHECK {
                return Err(Error::UnsupportedStep(step));
            }
            let reply = self.send(portal, &Call::auth_check(), random, now)?;
            match Step::resolve(&NextStep::parse(&reply.response)?) {
                Some(next) => step = next,
                None => return Ok(()),
            }
        }
        Err(Error::TooManySteps)
    }
}

/// Whether the answer to the second-stage request passes (spec §3.6): it echoes `nonce`, its
/// code is not one of the two refusals, and its signature header matches its body under
/// `key`.
fn second_stage_passes(key: &SignKey, response: &Response, nonce: &str) -> Result<bool, Error> {
    let Ok(reply) = AntiMitmReply::parse(response) else {
        return Ok(false);
    };
    let refused = reply
        .code
        .is_some_and(|code| SECOND_STAGE_REFUSALS.contains(&code));
    Ok(reply.nonce == nonce && !refused && key.verify(reply.body, &reply.signature)?)
}
