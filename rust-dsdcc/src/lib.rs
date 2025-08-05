#[cxx::bridge(namespace = "rust_dsdcc")]
mod ffi {
    #[repr(u32)]
    enum DSDDecodeMode {
        DSDDecodeDMR,
        DSDDecodeNXDN48,
        DSDDecodeNXDN96,
    }

    unsafe extern "C++" {
        include!("rust_dsdcc.h");

        type DSDDecoder;
        type DSDDecodeMode;

        fn run(self: &DSDDecoder, sample: i16);
        fn setQuiet(self: &DSDDecoder);
        fn setDecodeMode(self: &DSDDecoder, mode: DSDDecodeMode, on: bool);

        fn create_dsddecoder() -> UniquePtr<DSDDecoder>;
    }
}

pub struct DSDDecoder {
    internal: cxx::UniquePtr<ffi::DSDDecoder>,
}

pub enum DSDDecodeMode {
    DSDDecodeDMR,
    DSDDecodeNXDN48,
    DSDDecodeNXDN96,
}

impl DSDDecoder {
    pub fn new() -> Self {
        DSDDecoder {
            internal: ffi::create_dsddecoder(),
        }
    }

    pub fn run(&self, sample: i16) {
        self.internal.run(sample);
    }

    pub fn set_quiet(&self) {
        self.internal.setQuiet();
    }

    pub fn set_decode_mode(&self, mode: DSDDecodeMode, on: bool) {
        let mode = match mode {
            DSDDecodeMode::DSDDecodeDMR => ffi::DSDDecodeMode::DSDDecodeDMR,
            DSDDecodeMode::DSDDecodeNXDN48 => ffi::DSDDecodeMode::DSDDecodeNXDN48,
            DSDDecodeMode::DSDDecodeNXDN96 => ffi::DSDDecodeMode::DSDDecodeNXDN96,
        };
        self.internal.setDecodeMode(mode, on);
    }
}
