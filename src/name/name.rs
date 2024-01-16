use derive_builder::Builder;

use crate::name::error::EncodingError;
use crate::name::label::{Label, NameLabel};

const COMPUTER_NOT_ALLOWED: &'static str = "\\/:*?\"<>|";

#[derive(Builder, Debug, Clone)]
pub struct Name {
    /// Preform all checks required by the RFCs. If set to false all optional soundness checks will
    /// be skipped when translating the name into its encoded formats.
    #[builder(default = "true")]
    check_soundness: bool,

    /// The netbios name.
    #[builder(setter(into))]
    pub name: String,

    /// The scope identifier.
    ///
    /// This is used as the suffix of the fully qualified netbios name.
    #[builder(default, setter(into, strip_option))]
    pub scope: Option<String>,

    /// The type of netbios name.
    ///
    /// This is used to signal to consuming software what the name is used for. This appends to
    #[builder(default, setter(into, strip_option))]
    pub ntype: Option<NameType>,
}

impl Name {
    pub fn encode(&self) -> Result<Vec<Label>, EncodingError> {
        match (self.check_soundness, self.ntype, &self.scope) {
            (true, t, s) => {
                let (name, scope) = check_soundness(&self.name, t, &s)?;

                let encoded_name = first_level_encode(&name)?;

                let mut labels = vec![NameLabel::new(encoded_name).unwrap().into()];
                if let Some((s1, s2)) = scope {
                    labels.push(NameLabel::new(s1.into()).unwrap().into());
                    labels.push(NameLabel::new(s2.into()).unwrap().into());
                }

                Ok(labels)
            }
            (false, Some(t), Some(s)) => {
                let encoded_name = first_level_encode(&format!("{}{}", self.name, t.to_string()))?;

                let mut labels = vec![NameLabel::new(encoded_name).unwrap().into()];
                for s in s.split('.') {
                    labels.push(NameLabel::new(s.into()).unwrap().into());
                }

                Ok(labels)
            }
            (false, Some(t), None) => {
                let encoded_name = first_level_encode(&format!("{}{}", self.name, t.to_string()))?;

                Ok(vec![NameLabel::new(encoded_name).unwrap().into()])
            }
            (false, None, Some(s)) => {
                let encoded_name = first_level_encode(&self.name)?;

                let mut labels = vec![NameLabel::new(encoded_name).unwrap().into()];
                for s in s.split('.') {
                    labels.push(NameLabel::new(s.into()).unwrap().into());
                }

                Ok(labels)
            }
            (false, None, None) => {
                let encoded_name = first_level_encode(&self.name)?;

                Ok(vec![NameLabel::new(encoded_name).unwrap().into()])
            }
        }
    }
}

fn check_soundness(
    name: &str,
    ntype: Option<NameType>,
    scope: &Option<String>,
) -> Result<(String, Option<(String, String)>), EncodingError> {
    let ntype = ntype.unwrap_or(NameType::PaddingSpace);

    name_checks(name)?;

    let fqdn_suffix = match scope {
        Some(s) => Some(scope_checks(&s)?),
        None => None,
    };

    Ok((format!("{:15}{}", name, ntype.to_string()), fqdn_suffix))
}

fn name_checks(name: &str) -> Result<(), EncodingError> {
    match name.len() {
        1..=15 => (),
        0 => return Err(EncodingError::EmptyName),
        _ => return Err(EncodingError::NameTooLong),
    }

    for c in name.chars() {
        if COMPUTER_NOT_ALLOWED.contains(c) {
            return Err(EncodingError::InvalidCharacter(c));
        }
    }

    Ok(())
}

fn scope_checks(scope: &str) -> Result<(String, String), EncodingError> {
    let fqdn_suffix: Vec<&str> = scope.split('.').collect();

    if fqdn_suffix.len() != 2 {
        return Err(EncodingError::InvalidScopeId);
    }

    Ok((fqdn_suffix[0].to_owned(), fqdn_suffix[1].to_owned()))
}

/// Converts a netbios name into a first level encoded name.
///
/// # Arguments
/// * `name` - The plaintext netbios name to encode.
///
/// # Example
/// ```
/// # use nbt::name::names::first_level_encode;
/// let name = "Workstation001";
/// let encoded = first_level_encode(name).unwrap();
///
/// assert_eq!(encoded, "FHGPHCGLHDHEGBHEGJGPGODADADBCACA");
/// ```
pub fn first_level_encode(name: &str) -> Result<String, EncodingError> {
    let padded = match name.len() {
        1..=16 => format!("{:16}", name),
        0 => return Err(EncodingError::EmptyName),
        _ => return Err(EncodingError::NameTooLong),
    };

    let mut enc = Vec::new();

    for v in padded.chars().map(|c| c as u8) {
        enc.push(0x41 + (v >> 4));
        enc.push(0x41 + (v & 15));
    }

    Ok(String::from_utf8(enc).unwrap())
}

/// converts a first level encode name into a netbios name.
///
/// # Arguments
/// * `name` - The first level encoded name to decode as bytes. ex: 'FHGPHCGLHDHEGBHEGJGPGODADADBCACA'
pub fn first_level_decode(name: &[u8]) -> Result<String, ()> {
    let mut bytes = Vec::new();

    for c in name.chunks(2) {
        let mut v = (c[0] - 0x41) << 4;
        v += c[1] - 0x41;
        bytes.push(v);
    }

    Ok(String::from_utf8(bytes).unwrap())
}

/// Convert a first level fully qualified domain name into a second level encoded list of labels.
///
/// This function should only be utilized after the first level encoding name has been joined with
/// the scope identifier. The scope identifier is the domain that the name is a part of the netbios
/// name.
///
/// To go from a human readable name to a list of labels, use the [encode] function instead.
///
/// # Arguments
/// * `name` - The fully qualified netbios name to encode. ex: 'FHGPHCGLHDHEGBHEGJGPGODADADBCACA.test.local'
pub fn second_level_encode(name: &str) -> Result<Vec<Label>, ()> {
    // Could check here to make sure the first part of the domain matches the shape of a first
    // level encoded name. it wouln't be perfect however it could provide some level of validation
    // that might be worth while.
    let mut labels = Vec::new();

    for label in name.split('.') {
        let label = NameLabel::new(label.into()).unwrap();
        labels.push(label.into());
    }

    Ok(labels)
}

pub fn decode(name: &[NameLabel]) -> Result<String, ()> {
    let mut bytes = Vec::new();

    for (idx, label) in name.iter().enumerate() {
        match idx {
            0 => {
                bytes.append(&mut first_level_decode(&label.name).unwrap().into_bytes());
            }
            _ => {
                bytes.push(b'.');
                bytes.append(&mut label.name.clone());
            }
        }
    }

    Ok(String::from_utf8(bytes).unwrap())
}

/// Convert a fully qualified netbios name into a list of labels.
///
/// # Arguments
/// * `name` - The fully qualified netbios name to encode. ex: 'Workstation001.test.local'
pub fn encode(name: &str) -> Result<Vec<Label>, ()> {
    Ok(second_level_encode(name)?)
}

/// The Type of Netbios Name
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum NameType {
    /// A Unique Name Type
    ///
    /// Used to associate the computer specified by name in Computer name and a single IP address in IP address for this static mapped entry.
    ///
    /// TODO: Sort out how this should be documented...
    /// When this type is selected, three types of records are statically added to the WINS database for the specified computer name.
    /// These are the (0x00) WorkStation, (0x03) Messenger, and (0x20) File Server types.
    Unique(UniqueName),

    /// A Group Name Type
    ///
    // Also referred to as a normal group.
    // This type is used to add a static entry for the computer, specified by name in a static mapping, to a workgroup used on your network.
    //
    // If this type is used, the IP address for the computer is not stored in WINS but is resolved through local subnet broadcasts.
    Group(GroupName),

    /// Indicates a domain name (0x1C) mapped entry for locating Windows NT domain controllers.
    Domain(DomainName),

    /// An Internet Name Type
    ///
    /// Used for special user-defined administrative groups. You can use this to group resources.
    ///
    /// For example, you can indicate a group of file or print servers for organizing shared resources that are visible when browsing your network places.
    ///
    /// Each Internet group is represented by a shared group name of (0x20) type in the WINS database.
    Internet(InternetName),

    /// A MultiHomed Name Type
    ///
    /// Used to register a unique name for a computer that has more than one IP address
    /// (either multiple adapters each using a single address or one network adapter configured with multiple IP addresses).
    MultiHomed,

    /// A Custom Name Type
    ///
    /// Any u8 value can be used as a custom name type. There is no defined standard for the name
    /// types. Conflicts are possible, and it is up to the consuming software to understand these
    /// types.
    Custom(u8),

    /// A Padding Space Convinence Type.
    PaddingSpace = 0x20,
}

impl NameType {
    pub fn to_string(&self) -> String {
        String::from_utf8(vec![(*self).into()]).unwrap()
    }
}

impl From<NameType> for String {
    fn from(ntype: NameType) -> String {
        ntype.to_string()
    }
}

impl From<NameType> for u8 {
    fn from(ntype: NameType) -> u8 {
        match ntype {
            NameType::Unique(ntype) => ntype as u8,
            NameType::Group(ntype) => ntype as u8,
            NameType::Domain(ntype) => ntype as u8,
            NameType::Internet(ntype) => ntype as u8,
            NameType::MultiHomed => 0x1F,
            NameType::Custom(ntype) => ntype,
            NameType::PaddingSpace => 0x20,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum UniqueName {
    /// Registered by the Workstation Service on the WINS client.
    ///
    /// In general, this name is called the NetBIOS computer name.
    Workstation = 0x00,

    AltMessenger = 0x01,

    /// This name type has two uses:
    /// # When sent with Computer Name:
    /// Registered by the Messenger service on the WINS client.
    ///
    /// This service is used by the client when sending and receiving messages.
    ///
    /// This name is usually appended to both the NetBIOS computer name for the WINS client computer
    /// and the name of the user currently logged on to that computer when sending messages on the network.
    ///
    /// # When sent with Username:
    /// Usernames for currently logged-on users are registered in the WINS database.
    ///
    /// Each username is registered by the Server service component so that users can receive any net send commands sent to the username.
    /// If more than one user logs on with the same username, only the first computer logged on with that name registers the name.
    Messenger = 0x03,

    /// Registered by the Routing and Remote Access service on the WINS client (when the service is started).
    RemoteAccessServer = 0x06,

    /// Registered by each Windows NT Server 4.0 operating system domain controller running as the domain master browser.
    ///
    /// This name record is used to allow remote browsing of domains.
    ///
    /// When a WINS server is queried for this name, a WINS server returns the IP address of the computer that registered this name.
    DomainMasterBrower = 0x1B,

    /// Registered for use by master browsers, of which there is only one per subnet.
    ///
    /// Backup browsers use this name to communicate with the master browser, retrieving the list of available servers from the master browser.
    ///
    /// WINS servers always return a positive registration response for domain_name(0x1D), even though the WINS server does not register this name in its database.
    /// Therefore, when a WINS server is queried for the domain_name(0x1D), the WINS server returns a negative response, which forces the client to broadcast for name resolution.
    MasterBrowser = 0x1D,

    /// Registered by the Network Dynamic Data Exchange (NetDDE) services.
    ///
    /// This appears only if the NetDDE services are started on the computer.
    NetworkDyanmicDataExchange = 0x1F,

    /// Registered by the Server service on the WINS client.
    ///
    /// This service is used to provide points of service to the WINS client to provide sharing of its files on the network.
    FileServer = 0x20,

    /// Registered by the RAS Client service on the WINS client (when the RAS Client is started).
    RemoteAccessClient = 0x21,
    ExchangeInterchange = 0x22,
    ExchangeStore = 0x23,
    ExchangeDirectory = 0x24,
    ModemSharingServer = 0x30,
    ModemSharingClient = 0x31,
    MacafeeAntiVirus = 0x42,
    SMSClientsRemoteControl = 0x43,
    SMSAdministratorsRemoteControlTool = 0x44,
    SMSClientsRemoteChat = 0x45,
    SMSClientsRemoteTransfer = 0x46,
    DECPathworks = 0x4C,
    AltDECPathworks = 0x52,
    ExchangeIMC = 0x6A,
    ExchangeMTA = 0x87,

    /// Registered by the Network Monitoring Agent Service and appearing only if the service is started on the WINS client computer.
    ///
    /// If the computer name has fewer than 15 characters, the remaining character spaces are padded with plus (+) symbols.
    NetworkMonitorAgent = 0xBE,

    /// Registered by the Network Monitoring Utility (included with Microsoft Systems Management Server).
    ///
    /// If the computer name has fewer than 15 characters, the remaining character spaces are padded with plus (+) symbols.
    NetworkMonitorApplication = 0xBF,
}

#[derive(Debug, Clone, Copy)]
pub enum GroupName {
    /// Registered by the Workstation Service so that it can receive browser broadcasts from LAN Manager-based computers.
    Domain = 0x00,

    /// Registered by the master browser for each subnet.
    ///
    /// When a WINS server receives a name query for this name, the WINS server always returns the network broadcast address for local network of the requesting client.
    MSBROWSER = 0x01,

    /// Registered for use by the domain controllers within the domain. These contain up to 25 IP addresses.
    DomainController = 0x1C,

    /// A normal group name.
    ///
    /// Any computers configured to be network browsers can broadcast to this name, and listen for broadcasts to this name, to elect a master browser.
    ///
    /// A statically mapped group name uses this name to register itself on the network.
    /// When a WINS server receives a name query for a name ending with (0x1E), the WINS server always returns the network broadcast address for the local network of the requesting client.
    /// The client can then use this address to broadcast to the group members.
    /// These broadcasts are for the local subnet and should not cross routers.
    BorwserElection = 0x1E,

    /// A special group name called the Internet Group is registered with WINS servers to identify groups of computers for administrative purposes.
    ///
    /// For example, "printersg" could be a registered group name used to identify an administrative group of print servers.
    Group = 0x20,
    IRISMULTICAST = 0x2F,
    IRISNAMESERVER = 0x33,
}

#[derive(Debug, Clone, Copy)]
pub enum DomainName {
    Domain = 0x00,
    MasterBrowser = 0x01,
    DomainController = 0x1C,
    BorwserElection = 0x1E,
}

#[derive(Debug, Clone, Copy)]
pub enum InternetName {
    InternetInformationServicesUnknown = 0x00,
    InternetInformationServicesGroup = 0x1C,
    DCAIrmaLanGatewayServerService = 0x20,
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn encode() {
        let name = NameBuilder::default()
            .name("TEST")
            .scope("domain.tld")
            .ntype(NameType::Unique(UniqueName::Workstation))
            .build()
            .unwrap();

        let encoded = name.encode().unwrap();

        println!("{:#?}", encoded);
    }
}
