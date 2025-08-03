#[cxx::bridge(namespace = "rust_dsdcc")]
mod ffi {
    unsafe extern "C++" {
        include!("rust_dsdcc.h");

        type DSDDecoder;

        fn run(self: &DSDDecoder, sample: i16);

        fn create_dsddecoder() -> Result<UniquePtr<DSDDecoder>>;
    }
}

pub struct DSDDecoder {
    internal: cxx::UniquePtr<ffi::DSDDecoder>,
}

impl DSDDecoder {
    pub fn new() -> Result<Self, cxx::Exception> {
        Ok(DSDDecoder {
            internal: ffi::create_dsddecoder()?,
        })
    }

    pub fn run(&self, sample: i16) {
        self.internal.run(sample);
    }
}
