use std::fmt;

use crate::ffi::{DSDStationType, DSDSyncType};

#[cxx::bridge()]
pub mod ffi {
    #[repr(u32)]
    #[derive(Debug)]
    pub enum DSDStationType {
        DSDStationTypeNotApplicable,
        DSDBaseStation,
        DSDMobileStation,
    }

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

    #[repr(u32)]
    #[derive(Debug)]
    pub enum DSDSyncType {
        DSDSyncP25p1P,
        DSDSyncP25p1N,
        DSDSyncX2TDMADataP,
        DSDSyncX2TDMAVoiceN,
        DSDSyncX2TDMAVoiceP,
        DSDSyncX2TDMADataN,
        DSDSyncDStarP,
        DSDSyncDStarN,
        DSDSyncNXDNP,
        DSDSyncNXDNN,
        DSDSyncDMRDataP,
        DSDSyncDMRDataMS,
        DSDSyncDMRVoiceP,
        DSDSyncDMRVoiceMS,
        DSDSyncProVoiceP,
        DSDSyncProVoiceN,
        DSDSyncNXDNDataP,
        DSDSyncNXDNDataN,
        DSDSyncDStarHeaderP,
        DSDSyncDStarHeaderN,
        DSDSyncDPMR,
        DSDSyncDPMRPacket,
        DSDSyncDPMRPayload,
        DSDSyncDPMREnd,
        DSDSyncYSF,
        DSDSyncNone,
    }

    unsafe extern "C++" {
        include!("rust_dsdcc.h");

        type DSDDecoder;
        type DSDDecodeMode;
        type DSDSyncType;
        type DSDStationType;

        fn run(self: &DSDDecoder, sample: i16);
        fn setQuiet(self: &DSDDecoder);
        fn setDecodeMode(self: &DSDDecoder, mode: DSDDecodeMode, on: bool);
        fn getSlot0Text(self: &DSDDecoder) -> String;
        fn getSlot1Text(self: &DSDDecoder) -> String;
        fn getSyncType(self: &DSDDecoder) -> DSDSyncType;
        fn getFrameTypeText(self: &DSDDecoder) -> String;
        fn getFrameSubtypeText(self: &DSDDecoder) -> String;
        fn getStationType(self: &DSDDecoder) -> DSDStationType;

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

    pub fn get_slot_0_text(&self) -> String {
        self.internal.getSlot0Text()
    }

    pub fn get_slot_1_text(&self) -> String {
        self.internal.getSlot1Text()
    }

    pub fn get_sync_type(&self) -> ffi::DSDSyncType {
        self.internal.getSyncType()
    }

    pub fn get_frame_type_text(&self) -> String {
        self.internal.getFrameTypeText()
    }

    pub fn get_frame_subtype_text(&self) -> String {
        self.internal.getFrameSubtypeText()
    }

    pub fn get_station_type(&self) -> ffi::DSDStationType {
        self.internal.getStationType()
    }
}

impl fmt::Display for DSDSyncType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for DSDStationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
