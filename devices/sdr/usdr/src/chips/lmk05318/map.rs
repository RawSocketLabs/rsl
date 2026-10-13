//! The register staging map: libusdr collects a configuration's registers by address
//! (`lmk05318_add_reg_to_map`), a later value for an address replacing the earlier, then
//! writes them in address order (`lmk05318_reg_wr_from_map`).
//!
//! libusdr keeps one map per process (`static registers_map`); here each chip carries its
//! own. Each configuration step clears it at the start or after writing, so a single chip
//! sees the same contents either way, including what a failed write leaves staged.

use super::reg::{IndexedLmk05318Register, Lmk05318Register, Reg};

/// How many addresses the map holds: 0x0000 to 0x019B (`LMK05318_REGADDR_MAX`).
const LEN: usize = Reg::LAST_STAGED as usize + 1;

/// Registers staged for writing, by address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RegisterMap {
    /// The staged byte per address, if any.
    staged: [Option<u8>; LEN],
}

impl RegisterMap {
    /// An empty map.
    pub(super) const fn new() -> Self {
        Self {
            staged: [None; LEN],
        }
    }

    /// Stages a typed register.
    pub(super) fn stage<R: Lmk05318Register>(&mut self, value: R) {
        self.stage_raw(R::ADDR, value.into());
    }

    /// Stages one copy of a repeated register.
    pub(super) fn stage_at<R: IndexedLmk05318Register>(&mut self, index: R::Index, value: R) {
        self.stage_raw(R::addr(index), value.into());
    }

    /// Stages a register as a byte.
    pub(super) fn stage_raw(&mut self, reg: Reg, value: u8) {
        self.staged[usize::from(u16::from(reg))] = Some(value);
    }

    /// The staged registers in address order.
    pub(super) fn staged(&self) -> impl Iterator<Item = (u16, u8)> + '_ {
        (0..)
            .zip(self.staged)
            .filter_map(|(addr, value)| Some((addr, value?)))
    }

    /// Empties the map (`lmk05318_registers_map_reset`).
    pub(super) fn clear(&mut self) {
        self.staged = [None; LEN];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_registers_come_out_in_address_order_with_the_last_value_kept() {
        let mut map = RegisterMap::new();
        map.stage_raw(Reg::Pll2Status, 1);
        map.stage_raw(Reg::DeviceControl, 2);
        map.stage_raw(Reg::Pll2Status, 3);
        assert_eq!(map.staged().collect::<Vec<_>>(), [(0x0c, 2), (0x16f, 3)]);
        map.clear();
        assert_eq!(map.staged().count(), 0);
    }
}
