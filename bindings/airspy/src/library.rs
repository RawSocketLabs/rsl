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
    /// The first version whose `airspy_stop_rx` always joins the callback
    /// threads: 1.0.10 skips the join once a USB failure cleared `streaming`,
    /// and 1.0.12 joins whenever a stop was requested. 1.0.11 is unverified.
    const STOP_ALWAYS_JOINS: Self = Self {
        major: 1,
        minor: 0,
        revision: 12,
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
    fn only_releases_from_1_0_12_always_join() {
        let version = |revision| LibVersion {
            major: 1,
            minor: 0,
            revision,
        };
        assert!(!version(10).stop_always_joins());
        assert!(!version(11).stop_always_joins());
        assert!(version(12).stop_always_joins());
    }
}
