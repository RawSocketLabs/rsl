use crate::ffi;

/// The libairspy C library version: major, minor and revision.
///
/// Ordered by major, then minor, then revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LibVersion {
    /// Major version; this crate supports 1.
    pub major: u32,
    /// Minor version.
    pub minor: u32,
    /// Revision; 1.0.6 is the oldest supported release.
    pub revision: u32,
}

impl LibVersion {
    /// The first release whose `airspy_stop_rx` always joins the callback
    /// threads. Through the v1.0.10 tag a USB failure clears `streaming`, which
    /// gates the join; upstream 3f23328 (after that tag, still reporting
    /// 1.0.10) sets `stop_requested` instead, and 1.0.11 ships it. 1.0.12 keys
    /// the join on `stop_requested`, which every stop sets. A snapshot built
    /// between 3f23328 and the 1.0.11 bump is treated as affected.
    const STOP_ALWAYS_JOINS: Self = Self {
        major: 1,
        minor: 0,
        revision: 11,
    };

    /// Whether a stop or close of this version always joins the callback
    /// threads, so state the callbacks read may be freed afterwards.
    pub(crate) fn stop_always_joins(self) -> bool {
        self >= Self::STOP_ALWAYS_JOINS
    }

    /// Fetch the version of the libairspy C library this process linked.
    #[must_use]
    pub fn linked() -> Self {
        let mut v = ffi::airspy_lib_version_t {
            major_version: 0,
            minor_version: 0,
            revision: 0,
        };
        ffi::airspy_lib_version(&mut v);

        Self {
            major: v.major_version,
            minor: v.minor_version,
            revision: v.revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_supported_libairspy() {
        let v = LibVersion::linked();
        assert_eq!(v.major, 1);
        let oldest = LibVersion {
            major: 1,
            minor: 0,
            revision: 6,
        };
        assert!(v >= oldest);
    }

    #[test]
    fn only_releases_from_1_0_11_always_join() {
        let version = |revision| LibVersion {
            major: 1,
            minor: 0,
            revision,
        };
        assert!(!version(10).stop_always_joins());
        assert!(version(11).stop_always_joins());
        assert!(version(12).stop_always_joins());
    }
}
