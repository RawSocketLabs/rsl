//! The cookies of one host.

use alloc::vec::Vec;

use crate::Cookie;

/// The cookies a client holds for **one scheme and host**, and the `Cookie` header they
/// make.
///
/// The jar knows nothing about hosts or schemes: a caller that talks to several keeps one
/// jar for each. Sharing one between `http` and `https` of a host would send a cookie the
/// server marked `Secure` in the clear, because that flag is not kept. It is bounded, so a server cannot grow it without limit: a cookie larger
/// than [`Self::MAX_COOKIE_LEN`] is not stored, and past [`Self::MAX_COOKIES`] the oldest
/// is dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Jar {
    /// In creation order, oldest first.
    cookies: Vec<Cookie>,
}

impl Jar {
    /// The most cookies kept.
    pub const MAX_COOKIES: usize = 64;
    /// The longest name, value and path, together, of a cookie that is kept (RFC 6265 §6.1
    /// asks for at least 4096 bytes).
    pub const MAX_COOKIE_LEN: usize = 4096;

    /// An empty jar.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cookies: Vec::new(),
        }
    }

    /// Takes in one `Set-Cookie` header value from the response to a request for
    /// `request_path`, at time `now` (seconds since the Unix epoch).
    ///
    /// A header that does not parse is ignored, as RFC 6265 §5.2 says. So is a cookie whose
    /// name, value or path contains a carriage return, line feed or NUL: such a cookie could
    /// not be sent back without breaking the header it travels in. A cookie that has already
    /// expired removes the one it names, which is how a server deletes a cookie. Cookies
    /// that have expired by `now` are dropped first, so they never hold a place a live one
    /// needs (§5.3).
    //~ deviates rfc6265#5.3 reason="Domain, Secure and HttpOnly are not stored: the jar is for one scheme and host, chosen by the caller"
    //~ deviates rfc6265#5.3 reason="a full jar drops its oldest cookie by creation, not by last access, which is not tracked"
    pub fn store(&mut self, set_cookie: &[u8], request_path: &[u8], now: i64) {
        self.cookies.retain(|held| !held.is_expired(now));
        let Some(cookie) = Cookie::parse(set_cookie, request_path, now) else {
            return;
        };
        let breaks_a_header =
            |bytes: &[u8]| bytes.iter().any(|byte| matches!(byte, b'\r' | b'\n' | 0));
        if [&cookie.name, &cookie.value, &cookie.path]
            .into_iter()
            .any(|part| breaks_a_header(part))
        {
            return;
        }
        if cookie.is_expired(now) {
            self.remove(&cookie.name, &cookie.path);
        } else {
            self.insert(cookie);
        }
    }

    /// Puts `cookie` in the jar, replacing one with the same name and path. The replaced
    /// cookie's place in the order is kept, as RFC 6265 §5.3 keeps its creation time. A
    /// cookie over [`Self::MAX_COOKIE_LEN`] is not stored. Nothing else is checked: this is
    /// the raw way in.
    pub fn insert(&mut self, cookie: Cookie) {
        if cookie.name.len() + cookie.value.len() + cookie.path.len() > Self::MAX_COOKIE_LEN {
            return;
        }
        let same = |held: &Cookie| held.name == cookie.name && held.path == cookie.path;
        if let Some(held) = self.cookies.iter_mut().find(|held| same(held)) {
            *held = cookie;
            return;
        }
        if self.cookies.len() == Self::MAX_COOKIES {
            self.cookies.remove(0);
        }
        self.cookies.push(cookie);
    }

    /// Removes the cookie with this name and path, if there is one.
    pub fn remove(&mut self, name: &[u8], path: &[u8]) {
        self.cookies
            .retain(|cookie| cookie.name != name || cookie.path != path);
    }

    /// The value of the `Cookie` header for a request for `request_path` at time `now`:
    /// every unexpired cookie whose path matches, as `name=value` joined by `; `, longer
    /// paths first and older cookies first among equals (RFC 6265 §5.4). `None` when no
    /// cookie applies, in which case the header is not sent.
    ///
    /// Names and values are written as stored: bytes, not necessarily UTF-8, and unchecked
    /// for anything put in with [`insert`](Self::insert).
    #[must_use]
    pub fn header(&self, request_path: &[u8], now: i64) -> Option<Vec<u8>> {
        let mut applicable: Vec<&Cookie> = self
            .cookies
            .iter()
            .filter(|cookie| !cookie.is_expired(now) && cookie.matches_path(request_path))
            .collect();
        // Stable, so creation order survives among paths of equal length.
        applicable.sort_by_key(|cookie| core::cmp::Reverse(cookie.path.len()));
        let mut pairs = applicable
            .iter()
            .map(|cookie| [cookie.name.as_slice(), b"=", cookie.value.as_slice()].concat());
        let first = pairs.next()?;
        Some(pairs.fold(first, |mut header, pair| {
            header.extend_from_slice(b"; ");
            header.extend(pair);
            header
        }))
    }

    /// The value of the unexpired cookie named `name`. When several paths hold one, the
    /// oldest is returned.
    #[must_use]
    pub fn get(&self, name: &[u8], now: i64) -> Option<&[u8]> {
        self.cookies
            .iter()
            .find(|cookie| cookie.name == name && !cookie.is_expired(now))
            .map(|cookie| cookie.value.as_slice())
    }

    /// Every cookie held, oldest first. One that has expired stays until the next
    /// [`store`](Self::store).
    #[must_use]
    pub fn cookies(&self) -> &[Cookie] {
        &self.cookies
    }
}
