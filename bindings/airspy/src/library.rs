use crate::ffi;

/// Hold major, minor and revision version numbers of libairspy C library.
///
/// Ordered by major, then minor, then revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LibVersion {
    pub major: u32,
    pub minor: u32,
    pub revision: u32,
}

impl LibVersion {
    /// Fetch the version of the libairspy C library this process linked.
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
}
