pub struct Client {
    pub smb_versions: Vec<Version>,
    pub selected_smb_version: Option<Version>,
    pub transport: Transport,
}

pub enum Transport {
    NetBios,
    Tcp,
}

pub enum Version {
    V1,
    V2,
    V3,
}
