#[cxx::bridge()]
pub mod ffi {
    #[repr(u32)]
    pub enum DSDDecodeMode {
        DSDDecodeAuto,
        DSDDecodeNone,
        DSDDecodeP25P1,
        DSDDecodeDStar,
        DSDDecodeNXDN48,
        DSDDecodeNXDN96,
        DSDDecodeProVoice,
        DSDDecodeDMR,
        DSDDecodeX2TDMA,
        DSDDecodeDPMR,
        DSDDecodeYSF,
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

    pub fn set_decode_mode(&self, mode: ffi::DSDDecodeMode, on: bool) {
        self.internal.setDecodeMode(mode, on);
    }
}
