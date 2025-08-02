#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("rust_dsdcc.h");

        type Decoder;

        fn new_decoder() -> UniquePtr<Decoder>;

        fn run(self: Pin<&mut Decoder>, sample: i16);

        fn get_frame_type_text(self: Pin<&mut Decoder>) -> &CxxString;
    }
}

pub use ffi::{new_decoder, Decoder};
