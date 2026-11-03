use crate::ffi;

/// Hold major, minor and revision version numbers of libairspy C library.
pub struct LibVersion {
    pub major: u32,
    pub minor: u32,
    pub revision: u32,
}

impl LibVersion {
    /// Fetch the current version of libairspy C library.
    pub fn get_version() -> Self {
        let mut v = ffi::airspy_lib_version_t {
            major_version: 0,
            minor_version: 0,
            revision: 0,
        };
        unsafe {
            ffi::airspy_lib_version(&mut v);
        }

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
    fn test_lib_version() {
        let v = LibVersion::get_version();
        assert!(v.major == 1);
        assert!((v.minor, v.revision) >= (0, 6));
    }
}
