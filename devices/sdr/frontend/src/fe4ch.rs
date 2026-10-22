//! The four-channel 400–7200 MHz front end (libusdr's `fe4ch4007200`,
//! `device/ext_fe_ch4_400_7200/ext_fe_ch4_400_7200.c`), built for the sSDR and dSDR: its
//! identification and bring-up (`ext_fe_ch4_400_7200_init`).
//!
//! libusdr applies no compatibility filter, so its probe tries this board on every
//! device, last. The board's chips sit on FPGA I2C core 1 (buses 0 and 1): a TMP114
//! sensor, four TCA6424A expanders and a `DACx0501`. A device with one I2C core refuses
//! the first of those transfers in its transport (the uSDR's do, with `-EINVAL`), which
//! libusdr reports as a failed board. So the bring-up below runs only on a transport that
//! passes core 1: no device this crate serves declares a second core (the `LSDr` declares
//! one, `DNLL_I2C_COUNT` 1), and only the `LSDr` oracle's shim and sim bus pass it.
//!
//! The bring-up checks the sensor's ID, writes a test vector to every expander's outputs
//! and reads it back, makes the expanders' pins outputs, drives them from the default
//! channel settings (`ext_fe_update_user`: every channel off, the automatic filter bank's
//! first band, the RX antenna for RX and the TRX antenna for TX), identifies the DAC and
//! reads the sensor's unique ID and temperature, which libusdr only logs. The board's
//! controls (`/debug/hw/exfe10_4ch_*`, `/dm/sensor/temp_fe0`, the channel, gain and
//! frequency calls sSDR/dSDR drivers make) are not ported: no device this crate serves
//! calls them.

use deps::tracing::{debug, warn};
use hal::{Bus, I2cAddr};

use crate::chips::dac80501::{Dac80501, Identity};
use crate::chips::tca6424a::{Reg, Tca6424a};
use crate::chips::tmp114::Tmp114;
use crate::error::Error;
use crate::gpio::{FrontEndGpio, GpioCommand, PinMode};

/// The connector setup: the third I2C bus's clock, then the fourth's data and clock, to
/// the core's second alternate function (`GPIO_I2C3_SCL`, `GPIO_I2C4_SDA`,
/// `GPIO_I2C4_SCL`).
const SETUP: [GpioCommand; 3] = [
    GpioCommand::Configure(6, PinMode::Alternate1),
    GpioCommand::Configure(1, PinMode::Alternate1),
    GpioCommand::Configure(5, PinMode::Alternate1),
];

/// The 1PPS pin to its first alternate function, once the sensor identified the board
/// (`GPIO_1PPS`).
const PPS: [GpioCommand; 1] = [GpioCommand::Configure(2, PinMode::Alternate0)];

/// The temperature sensor U69, a TMP114NB on core 1, bus 0 (`I2C_TEMP_U69`).
const SENSOR: Tmp114 = Tmp114::at(I2cAddr::on_core(1, 0, 0x4e));

/// The DAC on core 1, bus 0 (`I2C_DAC`).
const DAC: Dac80501 = Dac80501::at(I2cAddr::on_core(1, 0, 0x48));

/// U114, bus 0 at the TCA6424A's low address: the RX filter switches
/// (`I2C_TCA6424AR_U114`).
const U114: Tca6424a = Tca6424a::at(I2cAddr::on_core(1, 0, 0x22));
/// U110, bus 0 at the high address: supplies, LEDs and RX enables (`I2C_TCA6424AR_U110`).
const U110: Tca6424a = Tca6424a::at(I2cAddr::on_core(1, 0, 0x23));
/// U300, bus 1 at the low address: channels A and B (`I2C_TCA6424AR_U300`).
const U300: Tca6424a = Tca6424a::at(I2cAddr::on_core(1, 1, 0x22));
/// U301, bus 1 at the high address: channels C and D (`I2C_TCA6424AR_U301`).
const U301: Tca6424a = Tca6424a::at(I2cAddr::on_core(1, 1, 0x23));

/// The four expanders in libusdr's order, each with the test vector its outputs must read
/// back (`TEST_OUT_U114`, `_U110`, `_U300`, `_U301`).
const EXPANDERS: [(Tca6424a, u16); 4] = [
    (U114, 0xf070),
    (U110, 0x00e0),
    (U300, 0x001e),
    (U301, 0x003c),
];

/// How many expander control registers the board's state drives (`FE_CTRL_REGS`).
const CONTROL_REGS: usize = 10;

/// The automatic filter-bank flag of a channel's filter selection (`RX_FB_AUTO`).
const FILTER_AUTO: u8 = 8;

/// One channel's settings (`struct fe_echan_config`, less the frequency the automatic
/// filter bank would follow).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Channel {
    /// The RX filter bank, `RX_FB_AUTO` possibly set (`rx_fb_sel`).
    filter: u8,
    /// The RX attenuator's code, 0 to 15 (`rx_dsa`).
    attenuation: u8,
    /// The TX amplifier's second stage bypassed (`tx_ss`).
    pa_bypass: bool,
    /// The TX path enabled on the device side (`tx_en`).
    tx: bool,
    /// The RX path enabled on the device side (`rx_en`).
    rx: bool,
}

impl Channel {
    /// The bring-up's settings: the automatic filter bank on its first band, no
    /// attenuation, the RX and TRX antennas, a single-stage amplifier, both paths off.
    const DEFAULT: Self = Self {
        filter: FILTER_AUTO,
        attenuation: 0,
        pa_bypass: false,
        tx: false,
        rx: false,
    };

    /// The filter bank's output and input switch codes (`_ext_fe_fbank_map`).
    const fn filter_switches(self) -> (u32, u32) {
        match self.filter & !FILTER_AUTO {
            0 => (3, 3), // 50–1000 MHz
            1 => (4, 2), // 1000–2000 MHz
            2 => (5, 1), // 2000–3500 MHz
            3 => (1, 5), // 2500–5000 MHz
            4 => (2, 4), // 3500–7100 MHz
            _ => (6, 6), // muted
        }
    }

    /// The switch, LED and amplifier values the antenna setting gives
    /// (`_ext_fe_antenna_sw_map_exp`) for `ANT_RX_TRX`: RX to the RX antenna, TX to the
    /// TRX antenna.
    // shortcut: only ANT_RX_TRX, the bring-up's; the other settings (`ant_sel`) arrive
    // with the board's controls (libusdr's ANT_HW_TDD leaves three values uninitialised).
    const fn antenna_switches(self) -> Switches {
        Switches {
            tdd_fdd: 0b11,                        // EXP_TDDFDD_P3_ANT_RX
            rx_tx: 0,                             // EXP_RXTX_SW_P1_TX
            tx_on_off: 0,                         // EXP_TX_ONOFF_P1_TRX_SW
            trx_led: if self.tx { 3 } else { 1 }, // LED_TRX_TXO, LED_TRX_OFF
            rx_led: !self.rx,                     // LED_RX_OFF when off
            tx_amp: self.tx,
        }
    }
}

/// What one channel's antenna setting drives.
#[derive(Clone, Copy, Debug)]
struct Switches {
    /// The two-bit TDD/FDD switch (`exp_tddfdd`).
    tdd_fdd: u32,
    /// The RX/TX switch (`exp_rxtx`).
    rx_tx: u32,
    /// The TX on/off switch (`exp_tx_onoff`).
    tx_on_off: u32,
    /// The two-bit TRX LED (`exp_led_trx`).
    trx_led: u32,
    /// The RX LED off (`exp_led_rx`, active low).
    rx_led: bool,
    /// The TX amplifier enabled (`act_tx`).
    tx_amp: bool,
}

/// Places `value`'s three low bits at `bits`, most significant first, as the generated
/// header's scattered fields do (`"8,23,22"`: bit 2 to 8, bit 1 to 23, bit 0 to 22).
const fn scatter(value: u32, bits: [u32; 3]) -> u32 {
    (value >> 2 & 1) << bits[0] | (value >> 1 & 1) << bits[1] | (value & 1) << bits[2]
}

/// The four-channel 400–7200 MHz front end, with the state libusdr keeps for it
/// (`struct ext_fe_ch4_400_7200`): the board-wide enables and each channel's settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fe4ch4007200 {
    /// The GPS module enabled (`ref_gps`).
    gps: bool,
    /// The IF bypass enabled (`if_vbyp`).
    if_bypass: bool,
    /// Channels A to D (`ucfg`).
    channels: [Channel; 4],
    /// The sensor's unique ID, read at bring-up (libusdr logs it as the board ID).
    board_id: u64,
}

impl Fe4ch4007200 {
    /// The sensor's 48-bit unique ID, which libusdr logs as the board ID.
    #[must_use]
    pub const fn board_id(&self) -> u64 {
        self.board_id
    }

    /// Looks for the board and brings it up (`ext_fe_ch4_400_7200_init`); `None` when it
    /// is absent: the sensor's ID is wrong, an expander fails its test vector, or the DAC
    /// reads absent (libusdr's `-ENODEV`, which its DAC check passes on although it only
    /// logs the failure).
    ///
    /// # Errors
    ///
    /// Any bus failure, a device refusing I2C core 1 included; [`Error::Board`] when the
    /// DAC reports an ID libusdr does not know (its `-EIO`).
    pub(crate) fn init(bus: &mut dyn Bus) -> Result<Option<Self>, Error> {
        FrontEndGpio::run(bus, &SETUP)?;
        let identified = SENSOR.identifies(bus)?;
        // libusdr logs these; nothing else reads them.
        for (expander, _) in EXPANDERS {
            expander.read16(bus, Reg::Config0)?;
        }
        if !identified {
            return Ok(None);
        }

        FrontEndGpio::run(bus, &PPS)?;
        U114.write16(bus, Reg::Output0, EXPANDERS[0].1)?;
        U114.write8(bus, Reg::Output0, 2, 0)?;
        U110.write16(bus, Reg::Output0, EXPANDERS[1].1)?;
        U110.write8(bus, Reg::Output0, 2, 0)?;
        U300.write16(bus, Reg::Output0, EXPANDERS[2].1)?;
        U301.write16(bus, Reg::Output0, EXPANDERS[3].1)?;
        let mut passed = true;
        for (expander, vector) in EXPANDERS {
            passed &= expander.read16(bus, Reg::Output0)? == vector;
        }
        U114.write16(bus, Reg::Output0, 0)?;
        U110.write16(bus, Reg::Output0, 0)?;
        if !passed {
            warn!("fe4ch4007200 expander test vectors failed");
            return Ok(None);
        }

        for expander in [U114, U110] {
            expander.make_outputs(bus)?;
        }
        let mut board = Self {
            gps: true,
            if_bypass: true,
            channels: [Channel::DEFAULT; 4],
            board_id: 0,
        };
        board.update(bus)?;
        for expander in [U300, U301] {
            expander.make_outputs(bus)?;
        }

        match DAC.init(bus)? {
            Identity::Known => {}
            Identity::Absent => {
                warn!("fe4ch4007200's DAC is not recognised");
                return Ok(None);
            }
            Identity::Unknown => {
                return Err(Error::Board {
                    board: "fe4ch4007200",
                    reason: "its DAC reports an unknown ID",
                });
            }
        }
        board.board_id = SENSOR.unique_id(bus)?;
        let celsius = SENSOR.celsius(bus)?;
        debug!(board_id = board.board_id, celsius, "fe4ch4007200 up");
        Ok(Some(board))
    }

    /// The expander control registers the settings give (`ext_fe_update_user`'s
    /// `fe_exp_regs`), each the 24 data bits libusdr's `MAKE_*` macros pack.
    fn registers(&self) -> [u32; CONTROL_REGS] {
        let [a, b, c, d] = self.channels;
        let (out_a, in_a) = a.filter_switches();
        let (out_b, in_b) = b.filter_switches();
        let (out_c, in_c) = c.filter_switches();
        let (out_d, in_d) = d.filter_switches();
        let switches = self.channels.map(Channel::antenna_switches);
        let any_rx = self.channels.iter().any(|channel| channel.rx);
        let any_tx = self.channels.iter().any(|channel| channel.tx);

        let mut regs = [0; CONTROL_REGS];
        // SW_RX_FILTER: each channel's input and output switch, bits scattered.
        regs[0] = scatter(in_a, [8, 23, 22])
            | scatter(in_b, [9, 21, 20])
            | scatter(in_c, [10, 19, 18])
            | scatter(in_d, [11, 17, 16])
            | scatter(out_a, [15, 6, 7])
            | scatter(out_b, [14, 4, 5])
            | scatter(out_c, [13, 3, 2])
            | scatter(out_d, [12, 0, 1]);
        // ENABLE: the RX and TX supplies, GPS, IF bypass, then each channel's PA bypass.
        regs[1] = u32::from(any_rx)
            | u32::from(any_tx) << 1
            | u32::from(self.gps) << 2
            | u32::from(self.if_bypass) << 3;
        // LED_TRX_CTRL: two bits a channel; LEDRX_CH_CTRL: RX enables, then RX LEDs.
        for (ch, (channel, switch)) in self.channels.iter().zip(switches).enumerate() {
            regs[1] |= u32::from(channel.pa_bypass) << (4 + ch);
            regs[2] |= (switch.trx_led & 0b11) << (2 * ch);
            regs[3] |= u32::from(channel.rx) << ch | u32::from(switch.rx_led) << (4 + ch);
        }
        // P_A_EN, ATTN_RX_CH and SW for each pair (A and B, then C and D).
        for (pair, regs) in regs[4..].chunks_exact_mut(3).enumerate() {
            let (first, second) = (2 * pair, 2 * pair + 1);
            let (one, two) = (switches[first], switches[second]);
            regs[0] = u32::from(one.tx_amp) << 7 | u32::from(two.tx_amp) << 6;
            regs[1] = u32::from(self.channels[first].attenuation & 0xf) << 4
                | u32::from(self.channels[second].attenuation & 0xf);
            regs[2] = two.rx_tx << 6
                | two.tx_on_off << 5
                | one.rx_tx << 3
                | one.tx_on_off << 2
                | (one.tdd_fdd >> 1 & 1) << 4
                | (one.tdd_fdd & 1) << 1
                | (two.tdd_fdd >> 1 & 1) << 7
                | (two.tdd_fdd & 1);
        }
        regs
    }

    /// Drives the expanders from the settings (`ext_fe_update_user`), register by
    /// register (`_ext_fe_ch4_400_7200_exp_upd`), stopping at the first failure.
    fn update(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        /// Where registers 1 to 9 go: three output ports each of U110, U300 and U301.
        const PORTS: [(Tca6424a, u8); CONTROL_REGS - 1] = [
            (U110, 0),
            (U110, 1),
            (U110, 2),
            (U300, 0),
            (U300, 1),
            (U300, 2),
            (U301, 0),
            (U301, 1),
            (U301, 2),
        ];
        let regs = self.registers();
        let [low, high, top, _] = regs[0].to_le_bytes();
        U114.write16(bus, Reg::Output0, u16::from_le_bytes([low, high]))?;
        U114.write8(bus, Reg::Output0, 2, top)?;
        for ((expander, port), value) in PORTS.into_iter().zip(&regs[1..]) {
            // Each register's low byte: libusdr's `uint8_t` argument truncates the rest.
            let [low, ..] = value.to_le_bytes();
            expander.write8(bus, Reg::Output0, port, low)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bring_up_settings_give_libusdrs_register_image() {
        // Worked from the generated `MAKE_EXT_FE_CH4_400_7200_E_*` macros: every filter
        // switch on 50–1000 MHz (code 3 in and out), GPS and IF bypass on, every TRX LED
        // off (1) and every RX LED off, both pairs' TDD/FDD switches on the RX antenna.
        let board = Fe4ch4007200 {
            gps: true,
            if_bypass: true,
            channels: [Channel::DEFAULT; 4],
            board_id: 0,
        };
        assert_eq!(
            board.registers(),
            [0xff_00ff, 0x0c, 0x55, 0xf0, 0, 0, 0x93, 0, 0, 0x93]
        );
    }

    #[test]
    fn enabled_channels_light_their_leds_and_power_their_paths() {
        let mut board = Fe4ch4007200 {
            gps: false,
            if_bypass: false,
            channels: [Channel::DEFAULT; 4],
            board_id: 0,
        };
        board.channels[0].rx = true;
        board.channels[3].tx = true;
        board.channels[3].attenuation = 0x1f;
        board.channels[1].filter = 4;
        let regs = board.registers();
        // Channel B's 3500–7100 MHz: in 4 (bit 2 to 9), out 2 (bit 1 to 4).
        assert_eq!(regs[0] & 0x30_4230, 0x200 | 0x10);
        assert_eq!(regs[1], 0b11, "both supplies, nothing else");
        assert_eq!(regs[2], 0b11_01_01_01, "D's LED shows TX");
        assert_eq!(regs[3], 0b1110_0001, "A's RX enabled, its LED on");
        assert_eq!(regs[7], 1 << 6, "D's amplifier");
        assert_eq!(regs[8], 0xf, "D's attenuator, masked to four bits");
    }
}
