//! TI LMK04832 clock generator, fitted on dual-ADC boards: libusdr's presence test
//! (`lmk04832_reset_and_test`, `hw/lmk04832/lmk04832.c`) as a sans-I/O sequence, and what
//! the sample rate asks of it: the PLL2 plan and layout (`lmk04832_configure_layout`,
//! [`ClockPlan`]), the two ADC clock ports (`lmk04832_configure_dports`) and the lock
//! readback (`lmk04832_check_lock`, [`lock_read`]).
//!
//! Frame, as `lmk04832.c` builds the SPI word: 24 bits, bit 23 set for a read
//! (`MAKE_LMK04832_RDREG`), the 15-bit register address in bits 22:8, data in bits 7:0; a
//! read keeps the low byte of the word returned. The generated header and YAML put the read
//! bit at 31 instead; the presence test does not use them. Register and field names are the
//! vendored YAML's; its 15-bit addresses do not fit `hal`'s 8-bit register traits,
//! so values here are plain bytes. Chip-level code: it moves to a shared
//! `devices/sdr/chips/lmk04832` crate when a second driver needs it (`PORTING.md`).

use bnb::{bitfield, u3};
use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::outcome::outcome;

/// The chip's name in errors.
pub(crate) const NAME: &str = "LMK04832";

/// Bit 23 of the frame: read.
const READ: u32 = 1 << 23;

/// The registers the presence test touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reg {
    /// See [`Reset`]. `RESET`, R0.
    Reset = 0x000,

    /// Product ID, the byte libusdr's `lmk04832_read16` puts low. `PROD_HI`, R4.
    ProductHigh = 0x004,

    /// Product ID, the byte libusdr puts high. `PROD_LOW`, R5.
    ProductLow = 0x005,

    /// Vendor ID, the byte libusdr puts low. `VNDR_HI`, R12.
    VendorHigh = 0x00c,

    /// Vendor ID, the byte libusdr puts high. `VNDR_LOW`, R13.
    VendorLow = 0x00d,

    /// See [`ResetConfig`]. `RESET_CFG`, R330.
    ResetConfig = 0x14a,
}

/// Resets the chip and selects its SPI mode. `RESET`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reset {
    /// Reset the chip; clears itself (YAML: "automatically cleared"). `RESET`, bit 7.
    #[bits(7..=7)]
    reset: bool,

    /// Four-wire SPI: 3-wire mode disabled. `SPI_3WARE_DIS` (sic), bit 4.
    #[bits(4..=4)]
    three_wire_disabled: bool,
}

/// What the RESET pin does. `RESET_CFG`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResetConfig {
    /// What the pin outputs, when [`Self::pin_type`] is an output (YAML). `RESET_MUX`,
    /// bits 5:3; [`SPI_READBACK`] routes register readback to it.
    #[bits(3..=5)]
    mux: u3,

    /// The pin's I/O type. `RESET_TYPE`, bits 2:0; [`OUTPUT_PUSH_PULL`] drives it.
    #[bits(0..=2)]
    pin_type: u3,
}

/// `RESET_MUX_SPI_READBACK`.
const SPI_READBACK: u3 = u3::new(6);

/// `RESET_TYPE_OUTPUT_PUSH_PULL`.
const OUTPUT_PUSH_PULL: u3 = u3::new(3);

/// The product ID libusdr accepts, `PROD_LOW << 8 | PROD_HI`.
const PRODUCT: u16 = 0xd163;

/// The vendor ID libusdr accepts, `VNDR_LOW << 8 | VNDR_HI`.
const VENDOR: u16 = 0x0451;

/// A write of `data` to `reg`.
fn write(target: SpiAddr, reg: Reg, data: u8) -> FpgaAccess {
    write_at(target, (reg as u16, data))
}

/// A write of the `(address, byte)` `entry` (`MAKE_LMK04832_REG_WR`).
pub(crate) fn write_at(target: SpiAddr, (addr, data): (u16, u8)) -> FpgaAccess {
    FpgaAccess::Spi {
        target,
        word: u32::from(addr) << 8 | u32::from(data),
        reg: addr.into(),
        write: true,
    }
}

/// A read of `reg`.
fn read(target: SpiAddr, reg: Reg) -> FpgaAccess {
    FpgaAccess::Spi {
        target,
        word: READ | (reg as u32) << 8,
        reg: reg as u32,
        write: false,
    }
}

/// The test's SPI exchanges, in libusdr's order: reset (with 4-wire SPI), route readback to
/// the RESET pin and drive it, release the reset (still 4-wire), then read the product and
/// vendor IDs, each low address first.
const STEPS: usize = 7;

/// `lmk04832_reset_and_test`: resets the chip, routes its register readback and reads its
/// IDs; the chip is present when they are libusdr's ([`Self::present`]). A missing chip is
/// not a failure (libusdr's `-ENODEV`, which open tolerates); a failed exchange is.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClockGeneratorProbe {
    /// The chip's SPI target.
    target: SpiAddr,

    /// Exchanges completed.
    done: usize,

    /// The ID bytes read, in [`Self::exchange`]'s read order.
    ids: [u8; 4],

    /// An exchange failed; nothing more is issued.
    failed: bool,
}

impl ClockGeneratorProbe {
    /// Starts the test on the chip at `target`; nothing has been accessed yet.
    pub(crate) const fn new(target: SpiAddr) -> Self {
        Self {
            target,
            done: 0,
            ids: [0; 4],
            failed: false,
        }
    }

    /// Exchange `step` of [`STEPS`].
    fn exchange(&self, step: usize) -> Option<FpgaAccess> {
        let target = self.target;
        let reset = |reset| {
            u8::from(
                Reset::new()
                    .with_reset(reset)
                    .with_three_wire_disabled(true),
            )
        };
        let routed = ResetConfig::new()
            .with_mux(SPI_READBACK)
            .with_pin_type(OUTPUT_PUSH_PULL);
        Some(match step {
            0 => write(target, Reg::Reset, reset(true)),
            1 => write(target, Reg::ResetConfig, routed.into()),
            2 => write(target, Reg::Reset, reset(false)),
            3 => read(target, Reg::ProductHigh),
            4 => read(target, Reg::ProductLow),
            5 => read(target, Reg::VendorHigh),
            6 => read(target, Reg::VendorLow),
            _ => return None,
        })
    }

    /// Whether the IDs read are the LMK04832's, as libusdr assembles them
    /// (`R(addr + 1) << 8 | R(addr)`); false until the test has finished.
    pub(crate) fn present(&self) -> bool {
        let [product_high, product_low, vendor_high, vendor_low] = self.ids;
        self.done == STEPS
            && u16::from_le_bytes([product_high, product_low]) == PRODUCT
            && u16::from_le_bytes([vendor_high, vendor_low]) == VENDOR
    }
}

impl Sequence for ClockGeneratorProbe {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.exchange(self.done).map(SequenceAction::Fpga)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let word = outcome(action, result, NAME).inspect_err(|_| self.failed = true)?;
        if let Some(id) = self.done.checked_sub(3).and_then(|at| self.ids.get_mut(at)) {
            *id = word.to_le_bytes()[0];
        }
        self.done += 1;
        Ok(())
    }
}

/// The status register `lmk04832_check_lock` reads twice (0x183, not in the vendored YAML);
/// libusdr takes bit 0 of the first read for the lock and only logs it.
const LOCK_STATUS: u16 = 0x183;

/// The read `lmk04832_check_lock` issues, twice (`MAKE_LMK04832_RDREG(0x183)`).
pub(crate) const fn lock_read(target: SpiAddr) -> FpgaAccess {
    FpgaAccess::Spi {
        target,
        word: READ | (LOCK_STATUS as u32) << 8,
        reg: LOCK_STATUS as u32,
        write: false,
    }
}

/// The internal VCOs' ranges in Hz (`VCO0_MIN`..`VCO1_MAX`), VCO0 first.
const VCOS: [(u32, u32); 2] = [
    (2_440_000_000, 2_580_000_000),
    (2_945_000_000, 3_255_000_000),
];

/// PLL2's highest phase-detector frequency (`PLL2_PD_FREQ_MAX`), in Hz.
const PLL2_PFD_MAX: u32 = 320_000_000;

/// PLL2's largest N divider (`PLL2_N_MAX`).
const PLL2_N_MAX: u32 = 262_143;

/// PLL2's prescaler range (`PLL2_PRESCALER_MIN`, `_MAX`).
const PLL2_PRESCALERS: core::ops::RangeInclusive<u32> = 2..=8;

/// The output ports the `LSDr` drives, one per ADC socket (`LMK04832_PORT_CLK0_1`,
/// `LMK04832_PORT_CLK12_13`): each port's registers sit 8 above the previous port's.
const ADC_PORTS: [u16; 2] = [0, 6];

/// How many writes [`ClockPlan::layout`] issues: 22 for the layout, 8 per ADC port.
pub(crate) const LAYOUT_WRITES: usize = 22 + 8 * ADC_PORTS.len();

/// Why [`ClockPlan::new`] has no plan; each is libusdr's failure before any access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlanFailure {
    /// No frequency, or above the fastest VCO (`-EINVAL`).
    OutOfRange,

    /// No multiple of the frequency falls in either VCO's range (`-ERANGE`).
    NoVco,

    /// No R, prescaler and N divider reach it from the input (`-EINVAL`).
    NoDividers,
}

/// What `lmk04832_configure_layout` programs on the `LSDr`'s route,
/// `LMK04832_CLKPATH_CLK0IN_PLL2` (`CLKin0` into PLL2, PLL1 off, an internal VCO
/// distributed), and what `lmk04832_configure_dports` then sets on both ADC ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClockPlan {
    /// VCO1 rather than VCO0 (`VCO_MUX`).
    vco1: bool,

    /// PLL2's R divider.
    r: u32,

    /// PLL2's prescaler, 2 to 8.
    prescaler: u32,

    /// PLL2's N divider.
    n: u32,

    /// The VCO's frequency, a multiple of the distributed one, in Hz.
    vco: u32,

    /// The distributed frequency, in Hz.
    distribution: u32,
}

impl ClockPlan {
    /// libusdr's plan to distribute `distribution` Hz from `input` Hz on `CLKin0` (the `LSDr`
    /// passes the sample rate as both): the first R from 1 whose phase-detector frequency
    /// is at most 320 MHz, VCO (0 first) and multiple `k` of the output in its range whose
    /// `output × R × k` lies within `R × k` of a multiple of the input, and prescaler from
    /// 2 dividing that multiple with N at most `PLL2_N_MAX`. Unsigned arithmetic wraps
    /// where libusdr's 32-bit arithmetic does.
    ///
    /// Only this route with a nonzero input is ported, the one call the `LSDr` makes;
    /// libusdr's search without an input and its other routes are not.
    pub(crate) fn new(input: u32, distribution: u32) -> Result<Self, PlanFailure> {
        if input == 0 || distribution == 0 || distribution > VCOS[1].1 {
            return Err(PlanFailure::OutOfRange);
        }
        let multiples = VCOS.map(|(min, max)| {
            let low = min.wrapping_add(distribution).wrapping_sub(1) / distribution;
            (low.max(1), max / distribution)
        });
        if multiples.iter().all(|(low, high)| low > high) {
            return Err(PlanFailure::NoVco);
        }
        for r in 1..512 {
            if input / r > PLL2_PFD_MAX {
                continue;
            }
            for (vco1, (&(low, high), &(vco_min, _))) in
                [false, true].into_iter().zip(multiples.iter().zip(&VCOS))
            {
                for k in low..=high {
                    if let Some(plan) = Self::try_multiple(input, distribution, r, k, vco1, vco_min)
                    {
                        return Ok(plan);
                    }
                }
            }
        }
        Err(PlanFailure::NoDividers)
    }

    /// The plan with R `r` and the VCO at `k` times the output, if libusdr takes it.
    fn try_multiple(
        input: u32,
        distribution: u32,
        r: u32,
        k: u32,
        vco1: bool,
        vco_min: u32,
    ) -> Option<Self> {
        let multiple = u64::from(distribution) * u64::from(r) * u64::from(k);
        let residual = multiple % u64::from(input);
        let slack = r.wrapping_mul(k);
        if residual > u64::from(slack) && residual < u64::from(input.wrapping_sub(slack)) {
            return None;
        }
        let pre_n = low32((multiple + u64::from(slack.wrapping_mul(2))) / u64::from(input));
        let vco = k.wrapping_mul(distribution);
        PLL2_PRESCALERS
            .filter(|prescaler| pre_n % prescaler == 0)
            .map(|prescaler| Self {
                vco1,
                r,
                prescaler,
                n: pre_n / prescaler,
                vco,
                distribution,
            })
            .find(|plan| plan.vco >= vco_min && plan.n <= PLL2_N_MAX)
    }

    /// The phase detector's input range code (`OSCIN_FREQ`) for the frequency the plan
    /// implies at PLL2's input, `VCO × R / prescaler / N`: above 255 MHz 4, above 127 MHz
    /// 2, above 63 MHz 1, else 0.
    fn input_range(self) -> u8 {
        let input =
            u64::from(self.vco) * u64::from(self.r) / u64::from(self.prescaler) / u64::from(self.n);
        match low32(input) / 1_000_000 {
            256.. => 4,
            128.. => 2,
            64.. => 1,
            _ => 0,
        }
    }

    /// The register writes of `lmk04832_configure_layout`, then of
    /// `lmk04832_configure_dports` on each ADC port, in libusdr's order, as
    /// `(address, byte)`.
    ///
    /// The layout: `VCO_BUF_CFG` (the VCO, `OSCout` off), `FB_CFG` 0x8E (PLL2's reference
    /// from `CLKin`, its N from the prescaler, PLL1's N from `OSCin`, the external
    /// feedback mux), `OSC_SYSREF_CFG` 0x90 (PLL1 and `OSCin` powered down, SYSREF on),
    /// `CLKIN_CFG` 0, `CLKIN_TYPE` 0x0E (`CLKin0` selected, into PLL1's demux; `CLKin1`
    /// off), each `CLKin`'s R divider 1, `PLL2_PD` 0x10 (PLL2 and its prescaler on), PLL2's
    /// R, prescaler with the input range, N twice (`PLL2_N_CAL`, then `PLL2_N`) and
    /// libusdr's constant 0x169 = 0x59 (`PLL2_DLD_EN`).
    ///
    /// Each port: the device-clock divider `VCO / output`, digital delay 8, the port
    /// powered and not high-speed, the device clock on output X, the SYSREF output
    /// powered down, X in LVDS and Y powered down.
    pub(crate) fn layout(self) -> [(u16, u8); LAYOUT_WRITES] {
        let r = self.r.to_le_bytes();
        let n = self.n.to_le_bytes();
        let prescaler = self.prescaler.to_le_bytes()[0];
        let pll2 = [
            (0x138, u8::from(self.vco1) << 5),
            (0x13f, 0x8e),
            (0x140, 0x90),
            (0x146, 0x00),
            (0x147, 0x0e),
            (0x153, 0x00),
            (0x154, 0x01),
            (0x155, 0x00),
            (0x156, 0x01),
            (0x157, 0x00),
            (0x158, 0x01),
            (0x173, 0x10),
            (0x160, r[1] & 0x0f),
            (0x161, r[0]),
            (
                0x162,
                (prescaler << 5) & 0xe0 | (self.input_range() << 2) & 0x1c,
            ),
            (0x163, n[2] & 0x03),
            (0x164, n[1]),
            (0x165, n[0]),
            (0x166, n[2] & 0x03),
            (0x167, n[1]),
            (0x168, n[0]),
            (0x169, 0x59),
        ];
        let divider = (self.vco / self.distribution).to_le_bytes();
        let port = |index: u16| {
            [
                (0x100, divider[0]),
                (0x101, 8),
                (0x102, divider[1] & 0x03),
                (0x103, 0x00),
                (0x104, 0x10),
                (0x105, 0x00),
                (0x106, 0x00),
                (0x107, 0x01),
            ]
            .map(|(addr, data)| (addr + 8 * index, data))
        };
        let mut writes = [(0, 0); LAYOUT_WRITES];
        let ports = ADC_PORTS.into_iter().flat_map(port);
        for (slot, entry) in writes.iter_mut().zip(pll2.into_iter().chain(ports)) {
            *slot = entry;
        }
        writes
    }
}

/// The low 32 bits of `value`, as libusdr's assignments to `unsigned` keep them.
const fn low32(value: u64) -> u32 {
    let [a, b, c, d, ..] = value.to_le_bytes();
    u32::from_le_bytes([a, b, c, d])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the test answering the four ID reads with `ids`; returns the words sent.
    fn run(probe: &mut ClockGeneratorProbe, ids: [u32; 4]) -> [u32; STEPS] {
        let mut words = [0; STEPS];
        let mut at = 0;
        while let Some(SequenceAction::Fpga(FpgaAccess::Spi { word, .. })) = probe.next() {
            words[at] = word;
            let answer = at.checked_sub(3).map_or(u32::MAX, |id| ids[id]);
            probe.complete::<()>(Ok(answer)).unwrap();
            at += 1;
        }
        words
    }

    #[test]
    fn the_test_resets_routes_readback_and_reads_the_ids_as_libusdr_does() {
        let mut probe = ClockGeneratorProbe::new(SpiAddr(2));
        let words = run(&mut probe, [0x63, 0xd1, 0x51, 0x04]);
        assert_eq!(
            words,
            [
                0x90, 0x01_4a33, 0x10, 0x80_0400, 0x80_0500, 0x80_0c00, 0x80_0d00
            ],
            "the words of MAKE_LMK04832_RESET, _RESET_CFG and _RDREG"
        );
        assert!(probe.present());
    }

    #[test]
    fn other_ids_or_a_floating_bus_are_an_absent_chip_not_a_failure() {
        let mut floating = ClockGeneratorProbe::new(SpiAddr(2));
        run(&mut floating, [u32::MAX; 4]);
        assert!(!floating.present());
        let mut swapped = ClockGeneratorProbe::new(SpiAddr(2));
        run(&mut swapped, [0xd1, 0x63, 0x51, 0x04]);
        assert!(!swapped.present(), "libusdr reads PROD_HI as the low byte");
    }

    #[test]
    fn fifty_mhz_is_libusdrs_plan_and_layout() {
        // From libusdr's /dm/rate/master = 50 MS/s on a dual-ADC board with lmk=1.
        let plan = ClockPlan::new(50_000_000, 50_000_000).expect("a plan");
        assert_eq!(
            (plan.vco1, plan.r, plan.prescaler, plan.n),
            (false, 1, 7, 7)
        );
        assert_eq!(plan.vco, 2_450_000_000);
        let words = plan
            .layout()
            .map(|(addr, data)| u32::from(addr) << 8 | u32::from(data));
        assert_eq!(
            words[..5],
            [0x01_3800, 0x01_3f8e, 0x01_4090, 0x01_4600, 0x01_470e]
        );
        assert_eq!(
            words[12..22],
            [
                0x01_6000, 0x01_6101, 0x01_62e0, 0x01_6300, 0x01_6400, 0x01_6507, 0x01_6600,
                0x01_6700, 0x01_6807, 0x01_6959
            ]
        );
        assert_eq!(
            words[22], 0x01_0031,
            "the first port divides 2450 MHz by 49"
        );
        assert_eq!(words[30], 0x01_3031, "the second port is CLKout12/13's");
        assert_eq!(words[37], 0x01_3701);
    }

    #[test]
    fn frequencies_no_vco_reaches_are_refused_as_libusdr_does() {
        assert_eq!(ClockPlan::new(1, 0), Err(PlanFailure::OutOfRange));
        assert_eq!(
            ClockPlan::new(1, 3_255_000_001),
            Err(PlanFailure::OutOfRange)
        );
        // 1.3 GHz: twice is past VCO0, three times past VCO1.
        assert_eq!(
            ClockPlan::new(1_300_000_000, 1_300_000_000),
            Err(PlanFailure::NoVco)
        );
    }

    #[test]
    fn the_lock_readback_reads_its_status_register() {
        assert!(matches!(
            lock_read(SpiAddr(2)),
            FpgaAccess::Spi {
                word: 0x81_8300,
                write: false,
                ..
            }
        ));
    }

    #[test]
    fn a_failed_exchange_ends_the_test_absent() {
        let mut probe = ClockGeneratorProbe::new(SpiAddr(2));
        probe.complete::<()>(Ok(0)).unwrap();
        let failure = probe.complete(Err(())).unwrap_err();
        assert_eq!(failure.access().map(|(chip, ..)| chip), Some(NAME));
        assert_eq!(probe.next(), None);
        assert!(!probe.present());
    }
}
