//! Which front-end boards libusdr knows, in its probe order, which connectors admit each
//! (`device/device_fe.c`'s `enum fe_type` and every board's compatibility check), and what
//! a device reports once one came up.

/// The connector a device offers its front ends: the compatibility string a libusdr
/// device passes to `device_fe_probe`. Each board admits only the connectors it fits; the
/// others make libusdr skip it without touching the bus.
///
/// The boards on the M.2 key A+E connector sit on I2C bus 1 of core 0 (libusdr's
/// `I2C_BUS_FRONTEND`, which the uSDR and XSDR both pass); the `LSDr` passes bus 0, but
/// admits no board that uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Connector {
    /// M.2 key A+E (`"m2a+e"`): the uSDR (`m2_lm6_1`) and the XSDR (`m2_lm7_1`).
    M2KeyAE,

    /// M.2 key B+M (`"m2b+m"`): the sSDR and sSDR Pro (`m2_lm7_1` with an LMS8001).
    M2KeyBM,

    /// The `LSDr`'s own connector (`"lsdr"`, `m2_da09_4_ad45_2`).
    Lsdr,
}

/// A front-end board libusdr can probe for (`enum fe_type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Board {
    /// The `PCIe` front end (`FET_PCIE_DEVBOARD`, `pciefe`).
    Pciefe,

    /// The XMASS synchronization board (`FET_PCIE_XMASS`, `xmass`).
    Xmass,

    /// The `SuperSync` clock board (`FET_PCIE_SUPER_SYNC`, `supersync`).
    SuperSync,

    /// The `SimpleSync` clock board (`FET_PCIE_SIMPLE_SYNC`, `simplesync`).
    SimpleSync,

    /// The M.2-to-`PCIe` breakout (`FET_PICE_BREAKOUT`, `exm2pe`).
    Exm2pe,

    /// The 100–5000 MHz front end (`FET_PCIE_FE1005000`, `fe1005000`).
    Fe1005000,

    /// The four-channel 400–7200 MHz front end (`FET_PCIE_FE4CH4007200`,
    /// `fe4ch4007200`).
    Fe4ch4007200,
}

impl Board {
    /// Every board, in the order libusdr tries them.
    pub const ALL: [Self; 7] = [
        Self::Pciefe,
        Self::Xmass,
        Self::SuperSync,
        Self::SimpleSync,
        Self::Exm2pe,
        Self::Fe1005000,
        Self::Fe4ch4007200,
    ];

    /// libusdr's name for the board (`s_fe_names`), as its `fe=` hints spell it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pciefe => "pciefe",
            Self::Xmass => "xmass",
            Self::SuperSync => "supersync",
            Self::SimpleSync => "simplesync",
            Self::Exm2pe => "exm2pe",
            Self::Fe1005000 => "fe1005000",
            Self::Fe4ch4007200 => "fe4ch4007200",
        }
    }

    /// Whether the board fits `connector`, as its init function's compatibility check says.
    /// A board that does not fit is skipped before any bus access (libusdr's `-ENODEV`).
    ///
    /// fe4ch4007200 has no check (it is commented out, "TODO sSDR/dSDR specific"), so
    /// libusdr tries it everywhere.
    #[must_use]
    pub const fn admits(self, connector: Connector) -> bool {
        // libusdr also accepts `"m2e"`, `"m2a"` and, for SuperSync, `"m2m"`; no device
        // passes them.
        match self {
            Self::Pciefe | Self::Xmass | Self::SuperSync | Self::SimpleSync | Self::Exm2pe => {
                matches!(connector, Connector::M2KeyAE)
            }
            Self::Fe1005000 => matches!(connector, Connector::Lsdr),
            Self::Fe4ch4007200 => true,
        }
    }
}

/// The front end a device brought up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrontEndKind {
    /// The `PCIe` front end.
    Pciefe,

    /// The M.2-to-`PCIe` breakout; `dac` says whether its oscillator DAC answered.
    Exm2pe {
        /// The oscillator DAC identified itself.
        dac: bool,
    },

    /// The XMASS synchronization board.
    Xmass,

    /// The `SuperSync` clock board.
    SuperSync,

    /// The `SimpleSync` clock board.
    SimpleSync,

    /// The 100–5000 MHz front end.
    Fe1005000,
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use super::*;

    /// The boards `connector` admits, in probe order.
    fn admitted(connector: Connector) -> Vec<Board> {
        Board::ALL
            .into_iter()
            .filter(|board| board.admits(connector))
            .collect()
    }

    #[test]
    fn each_connector_admits_the_boards_libusdrs_compatibility_checks_accept() {
        assert_eq!(
            admitted(Connector::M2KeyAE),
            [
                Board::Pciefe,
                Board::Xmass,
                Board::SuperSync,
                Board::SimpleSync,
                Board::Exm2pe,
                Board::Fe4ch4007200
            ]
        );
        assert_eq!(
            admitted(Connector::Lsdr),
            [Board::Fe1005000, Board::Fe4ch4007200]
        );
        assert_eq!(admitted(Connector::M2KeyBM), [Board::Fe4ch4007200]);
    }
}
