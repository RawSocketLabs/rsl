# rtlsdr — provenance ledger

Every register address, value, field, request code, identifier and sequence in `rtlsdr`
and `rtlsdr-sim` has a row here, and every such constant in code names its row with a
`prov: <ID>` doc tag. A value with no admissible source is a GAP row and, in code, a
`Pending` placeholder that refuses at runtime; nothing is guessed.

**Licensing rule (owner's, absolute).** Target licence MIT OR Apache-2.0. No GPL RTL-SDR
source or anything derived from one is read, searched or quoted, and recollection of such
sources is not evidence: librtlsdr/osmocom rtl-sdr and forks, the Linux kernel's
rtl28xxu/rtl2832/r820t/e4000/fc001x drivers, rtl-sdr-rs, pyrtlsdr internals, SDR++/gqrx.
Captures of librtlsdr's USB traffic are allowed but tagged GPLOBS and never a row's sole
source (`CAPTURES.md`).

## Classes

| Class | Meaning |
|---|---|
| DS | A vendor document, cited by document, page and table |
| DESC | A USB descriptor dump of an owner's dongle (`lsusb -v`, sysfs), cited by capture id |
| CAP | A usbmon/Wireshark capture of the vendor's Windows software, cited by capture id + sha256 |
| EXP | An experiment on the owner's hardware, cited by its recorded procedure and result |
| GPLOBS | A capture of librtlsdr's traffic: corroboration only, never the sole source |
| GAP | Not evidenced; the code holds a refusing placeholder or nothing |

## Documents

| Doc | Document | Identity |
|---|---|---|
| R820T2-RD | Rafael Microelectronics, *R820T2 Register Description*, VER1.3 2014-10-14, 11 pp. Held outside the repo (`references/rtlsdr-datasheets/`, provenance in its `SOURCES.md`); never committed | SHA-256 `52072cb9c86a522feb2b6a68b98518422afae9272b3393f34c27b3ae3695e68e` |

"T1-2" is the document's Table 1-2 (register matrix, p. 5); "T1-3" its Table 1-3
(register index and description, pp. 6-11).

## R820T2 rows

| ID | Rust item | Value | Meaning | Class | Source | Recorded | Corroboration |
|---|---|---|---|---|---|---|---|
| R820-DS-01 | `r820t2::registers::I2C_ADDRESS` | `0x1A` | 7-bit I2C slave address; write byte 0x34, read byte 0x35 | DS | R820T2-RD p. 2, §1.1, Table 1-1 | agent, 2026-10-19 | none yet (DESC/CAP of V4 pending; the V4 carries an R828D, `R820-GAP-11`) |
| R820-DS-02 | `registers::REGISTER_COUNT`; sim `tuner::REGISTER_COUNT` | 32 | registers R0-R31 | DS | R820T2-RD p. 5, §1.2 "Total 32 registers"; T1-2 rows 0x00-0x1F | agent, 2026-10-19 | none yet |
| R820-DS-03 | `Registers::page_write`; sim `R820t2::write` | first byte = register address, then consecutive registers | I2C write mode (page write) | DS | R820T2-RD p. 3, Figure 1-1 | agent, 2026-10-19 | none yet |
| R820-DS-04 | `Registers::from_read`; sim `R820t2::read` | read from R0 on; each byte LSB first, so an MSB-first master sees it bit-reversed | I2C read mode | DS | R820T2-RD p. 4, "Reading data transmission begins from core register 0 … transmitted from LSB to MSB" | agent, 2026-10-19 | none yet. The bit reversal is this crate's reading of "LSB to MSB" against I2C's normal MSB-first byte order (NXP UM10204, *I2C-bus specification and user manual*, §3.1.5 "Byte format"; not yet held under `references/`). The driver (`Registers::from_read`) and the sim (`R820t2::read`) share this one reading, so their cross-check cannot catch it; a capture of an R0 read (expect 0x69 on the wire) confirms or refutes it |
| R820-DS-05 | `registers::CHIP_ID`; sim `tuner::CHIP_ID` | `0x96` | R0's fixed content, the read-mode reference check point | DS | R820T2-RD p. 4 (text), T1-2 row R0 (B7..B0 = 1 0 0 1 0 1 1 0) | agent, 2026-10-19 | none yet |
| R820-DS-06 | `registers::FIRST_WRITABLE`; sim `tuner::FIRST_WRITABLE` | `0x05` | R0-R4 are read-only (T1-2 column R/W "R"); R5-R31 "W/R" | DS | R820T2-RD T1-2 p. 5; p. 3 calls R0-R4 "reserved for internal use only" yet "can be written by I2C write command": the code follows T1-2 and refuses such writes | agent, 2026-10-19 | none yet |
| R820-DS-07 | `field::VCO_INDICATOR`, `field::RF_INDICATOR` | R2[6:0], R3[7:0] | read-only indicators | DS | R820T2-RD T1-2 p. 5 | agent, 2026-10-19 | none; encodings `R820-GAP-09` |
| R820-DS-10 | `field::{PWD_LT, PWD_LNA1, LNA_GAIN_MODE, LNA_GAIN}`, `setting::LnaGainMode` | R5[7], [5], [4] (0 auto, 1 manual), [3:0] (0 min..15 max) | loop-through power, LNA1 power, LNA gain mode, LNA manual gain | DS | R820T2-RD T1-3 p. 6; T1-2 p. 5 | agent, 2026-10-19 | none yet |
| R820-DS-11 | `field::{PWD_PDET1, PWD_PDET3, FILT_3DB, PW_LNA}` | R6[7] (0 on), [6] (1 on), [5] (+3 dB), [2:0] (0 max..7 min) | detector powers, filter +3 dB, LNA power | DS | R820T2-RD T1-3 p. 6 | agent, 2026-10-19 | none yet |
| R820-DS-12 | `field::{PWD_MIX, PW0_MIX, MIXGAIN_MODE, MIX_GAIN}`, `setting::MixerGainMode` | R7[6] (1 on), [5], [4] (0 manual, 1 auto), [3:0] (0 min..15 max) | mixer power, current, gain mode, manual gain | DS | R820T2-RD T1-3 p. 6 | agent, 2026-10-19 | none yet |
| R820-DS-13 | `field::{PWD_AMP, PW0_AMP, IMR_G}` | R8[7] (1 on), [6], [5:0] (0..63) | mixer buffer power, current, image gain | DS | R820T2-RD T1-3 p. 7 | agent, 2026-10-19 | none yet |
| R820-DS-14 | `field::{PWD_IFFILT, PW1_IFFILT, IMR_P}` | R9[7] (0 on), [6], [5:0] (0..63) | IF filter power, current, image phase | DS | R820T2-RD T1-3 p. 7 | agent, 2026-10-19 | none yet |
| R820-DS-15 | `field::{PWD_FILT, PW_FILT, FILT_CODE}` | R10[7] (1 on), [6:5] (0 highest..3 lowest power), [3:0] (0 widest..15 narrowest) | channel filter power and fine bandwidth | DS | R820T2-RD T1-3 p. 7 | agent, 2026-10-19 | none; corners in Hz `R820-GAP-05` |
| R820-DS-16 | `field::{FILT_BW, HPF}`, `setting::FilterBandwidth` | R11[6:5] (00 widest, 01/10 middle, 11 narrowest), [3:0] (0 highest..15 lowest corner) | coarse bandwidth, high-pass corner | DS | R820T2-RD T1-3 p. 8 (T1-2 p. 5 names `FILT_BW[1:0]`) | agent, 2026-10-19 | none; whether 01 = 10 not stated, both kept |
| R820-DS-17 | `field::{PWD_VGA, VGA_MODE, VGA_CODE}`, `setting::VgaMode` | R12[6] (1 on), [4] (0 code, 1 `vagc` pin), [3:0] | IF VGA power, gain source, manual gain | DS | R820T2-RD T1-3 p. 8 (the text says `vga_code[5:0]` for a [3:0] field; bitmap used) | agent, 2026-10-19 | none yet |
| R820-DS-18 | `field::{LNA_VTH_H, LNA_VTH_L}` | R13[7:4], [3:0]: 0 = 0.34 V..15 = 1.94 V, ~0.1 V/step | LNA AGC detector thresholds | DS | R820T2-RD T1-3 p. 8 (named `LNA_VTHH[4:0]` for bits [7:4], `LANVTH_H` in T1-2; bitmap used) | agent, 2026-10-19 | none yet |
| R820-DS-19 | `field::{MIX_VTH_H, MIX_VTH_L}` | R14[7:4], [3:0]: as R13 | mixer AGC detector thresholds | DS | R820T2-RD T1-3 p. 8 (`[4:0]` name for bits [7:4]; bitmap used) | agent, 2026-10-19 | none yet |
| R820-DS-20 | `field::{CLK_OUT_ENB, CLK_AGC_ENB}` | R15[4], [1], 0 = on | clock-out pin, internal AGC clock | DS | R820T2-RD T1-3 p. 9 | agent, 2026-10-19 | none yet |
| R820-DS-21 | `field::{SEL_DIV, REF_DIV2, CAPX}`, `setting::{MixerDivider, RefDivider, XtalCap}` | R16[7:5]: 000 /2, 001 /4, 010 /8, 011 /1; [4]: 0 fref = xtal, 1 fref = xtal/2; [1:0]: 0/10/20/30 pF | VCO-to-mixer divider, reference divider, crystal load | DS | R820T2-RD T1-3 p. 9 (named `SEL_DIV[3:0]` for bits [7:5], `SEL_DIV[2:0]` in T1-2; bitmap used) | agent, 2026-10-19 | none; codes 100-111 `R820-GAP-03` |
| R820-DS-22 | `field::PW_LDO_A`, `setting::AnalogLdo` | R17[7:6]: 00 off, 01 2.1 V, 10 2.0 V, 11 1.9 V | PLL analog LDO | DS | R820T2-RD T1-3 p. 9 | agent, 2026-10-19 | none yet |
| R820-DS-23 | `field::PW_SDM` | R18[3] | named only | DS | R820T2-RD T1-2 p. 5; no T1-3 description | agent, 2026-10-19 | none; meaning not stated |
| R820-DS-24 | `field::{SI2C, NI2C}`, `pll::{NI2C_WEIGHT, NINT_OFFSET, NINT_MIN, NINT_MAX}` | R20[7:6], [5:0]; `Nint = 4·NI2C + SI2C + 13` | PLL integer divider inputs | DS | R820T2-RD T1-3 p. 9 (T1-2 names `N_I2C[4:0]` over bits B5..B0; T1-3 `NI2C[5:0]` bits [5:0]; six bits used) | agent, 2026-10-19 | none yet |
| R820-DS-25 | `field::{SDM_IN_LOW, SDM_IN_HIGH}`, `pll::SDM_BITS` | R21 = `SDM_IN[8:1]`, R22 = `SDM_IN[16:9]`; `Nfra = Σ SDM_IN[k]·2^(k-17)` = SDM/2^16 | PLL fractional divider input | DS | R820T2-RD T1-2 p. 5 (byte order); T1-3 p. 9 (formula; it labels both bytes `SDM_IN[7:0]`, so T1-2's order is used) | agent, 2026-10-19 | none yet |
| R820-DS-26 | `pll::NDIV_FACTOR`, `PllDivider::ndiv_q16` | `Ndiv = (Nint + Nfra)·2` | PLL divider number | DS | R820T2-RD T1-3 p. 9 | agent, 2026-10-19 | none; its relation to f_vco `R820-GAP-01` |
| R820-DS-27 | `field::{PW_LDO_D, OPEN_D}`, `setting::DigitalLdo` | R23[7:6]: 00 1.8 V 8 mA, 01 1.8 V 4 mA, 10 2.0 V 8 mA, 11 off; [3]: 0 high-Z, 1 low-Z | PLL digital LDO, open drain | DS | R820T2-RD T1-3 p. 10 | agent, 2026-10-19 | none yet |
| R820-DS-28 | `field::{PWD_RFFILT, SW_AGC}` | R25[7] (1 on), [4] (0 `agc_in`, 1 `agc_in2`) | RF filter power, AGC pin | DS | R820T2-RD T1-3 p. 10 | agent, 2026-10-19 | none yet |
| R820-DS-29 | `field::{RFMUX, PLL_AUTO_CLK, RFFILT}`, `setting::{RfMux, PllAutoClock, RfFilterBand}` | R26[7:6]: 00 TF on, 01 bypass; [3:2]: 128/32/8 kHz; [1:0]: 00 highest, 01 medium, 10 low band | tracking filter switch, PLL auto-tune clock, RF filter band | DS | R820T2-RD T1-3 p. 10 | agent, 2026-10-19 | none; other codes and band edges `R820-GAP-06` |
| R820-DS-30 | `field::{TF_NCH, TF_LP}` | R27[7:4], [3:0]: 0 highest..15 lowest corner | tracking filter notch and low-pass corners | DS | R820T2-RD T1-3 p. 10 (named `TF_NCH[1:0]` for bits [7:4]; bitmap used) | agent, 2026-10-19 | none; table vs frequency `R820-GAP-07` |
| R820-DS-31 | `field::PDET3_GAIN` | R28[7:4]: 0 highest..15 lowest | detector 3 take-off point | DS | R820T2-RD T1-3 p. 10 (named `[1:0]`; bitmap used) | agent, 2026-10-19 | none yet |
| R820-DS-32 | `field::{PDET1_GAIN, PDET2_GAIN}` | R29[5:3] (0 highest), [2:0] (0 lowest..7 highest) | detector 1/2 take-off points | DS | R820T2-RD T1-3 p. 11 (PDET1's text runs to "15: Lowest" for a 3-bit field) | agent, 2026-10-19 | none yet |
| R820-DS-33 | `field::{FILTER_EXT, PDET_CLK}` | R30[6] (1 enable), [5:0] (0 min..63 max) | weak-signal filter extension, detector timing | DS | R820T2-RD T1-3 p. 11 (T1-2 p. 5 shows R30 B6 as fixed 1 and names `PDET_CLK[4:0]`; T1-3 used) | agent, 2026-10-19 | none yet |
| R820-DS-34 | `gain::{VGA_MIN_TENTH_DB, VGA_STEP_TENTH_DB}`, `gain::VgaGain` | code 0 = -12.0 dB, 15 = +40.5 dB, 3.5 dB/step | IF VGA manual gain steps | DS | R820T2-RD T1-3 p. 8 (printed "-3.5dB/step"; the endpoints fix +3.5 dB per code) | agent, 2026-10-19 | none yet |
| R820-DS-35 | `setting::HALVE_XTAL_ABOVE_HZ`, `RefDivider::advised` | 24 MHz | REF_DIV2 = 1 advised "for Xtal > 24MHz" | DS | R820T2-RD T1-3 p. 9 | agent, 2026-10-19 | none yet |

## R820T2 gaps

| ID | Rust item | Value | Meaning | Class | Source | Recorded | Corroboration |
|---|---|---|---|---|---|---|---|
| R820-GAP-01 | none | ? | how `Ndiv` relates f_vco to fref (not stated) | GAP | needs a second document or CAP + EXP | agent, 2026-10-19 | — |
| R820-GAP-02 | none | ? | VCO range, and so `SEL_DIV` choice per LO frequency | GAP | needs document or EXP | agent, 2026-10-19 | — |
| R820-GAP-03 | `MixerDivider::from_bits` → `None` | ? | `SEL_DIV` codes 100-111 | GAP | not in R820T2-RD | agent, 2026-10-19 | — |
| R820-GAP-04 | none | ? | LNA_GAIN and MIX_GAIN steps in dB | GAP | needs document or EXP | agent, 2026-10-19 | — |
| R820-GAP-05 | none | ? | FILT_CODE, FILT_BW, HPF corners in Hz | GAP | needs document or EXP | agent, 2026-10-19 | — |
| R820-GAP-06 | `RfMux`/`PllAutoClock`/`RfFilterBand::from_bits` → `None` | ? | RFMUX 10/11, PLL_AUTO_CLK 11, RFFILT 11; RFFILT band edges | GAP | not in R820T2-RD | agent, 2026-10-19 | — |
| R820-GAP-07 | none | ? | TF_NCH/TF_LP settings per RF frequency | GAP | needs CAP or EXP | agent, 2026-10-19 | — |
| R820-GAP-08 | `Registers::page_write` refuses unread registers (`RegisterError::NotRead`); `Registers::default` knows none; the sim starts zero but R0 | ? | power-on defaults; whether T1-2's fixed 0/1 bits are required values or defaults; init values | GAP | needs CAP (vendor software's init writes) | agent, 2026-10-19 | — |
| R820-GAP-09 | none | ? | VCO_INDICATOR / RF_INDICATOR encodings (PLL lock) | GAP | needs document or EXP | agent, 2026-10-19 | — |
| R820-GAP-10 | none | ? | image (IMR) calibration procedure | GAP | needs CAP or EXP | agent, 2026-10-19 | — |
| R820-GAP-11 | none | ? | the R828D (Blog V4): I2C address, identity byte, register differences | GAP | no datasheet found (`SOURCES.md`); needs CAP of the V4 | agent, 2026-10-19 | — |

## RTL2832U and dongle gaps

No admissible RTL2832U source is held. These come from the owner's RTL-SDR Blog V4 (R828D)
and "V5" dongles: descriptor dumps (DESC) and captures (CAP), per `CAPTURES.md`.

| ID | Rust item | Value | Meaning | Class | Source | Recorded | Corroboration |
|---|---|---|---|---|---|---|---|
| RTL-GAP-01 | `rtl2832u::BLOG_V4_ID: Pending<UsbId>` | ? | Blog V4 VID:PID | GAP | needs DESC | agent, 2026-10-19 | — |
| RTL-GAP-02 | `rtl2832u::V5_ID: Pending<UsbId>` | ? | V5 VID:PID | GAP | needs DESC | agent, 2026-10-19 | — |
| RTL-GAP-03 | `rtl2832u::STREAM_INTERFACE: Pending<UsbInterface>` | ? | interface number and bulk IN endpoint of the sample stream | GAP | needs DESC | agent, 2026-10-19 | — |
| RTL-GAP-04 | none (sim takes it as `Wiring`) | ? | vendor `bmRequestType`/`bRequest` codes for register access | GAP | needs CAP | agent, 2026-10-19 | — |
| RTL-GAP-05 | none (sim takes it as `Wiring`) | ? | how a register's address and block map onto `wValue`/`wIndex` | GAP | needs CAP | agent, 2026-10-19 | — |
| RTL-GAP-06 | none (sim takes it as `Wiring::gate`) | ? | the I2C repeater gate's register and bit | GAP | needs CAP | agent, 2026-10-19 | — |
| RTL-GAP-07 | `rtl2832u::OPEN: Pending<()>`, `Device::open` | ? | the open: init order and values | GAP | needs CAP | agent, 2026-10-19 | — |
| RTL-GAP-08 | none (sim takes it as `Wiring::tuner_request`) | ? | how a tuner I2C transaction rides the vendor requests | GAP | needs CAP | agent, 2026-10-19 | — |
| RTL-GAP-09 | none | ? | the bulk stream's sample format | GAP | needs CAP + EXP | agent, 2026-10-19 | — |
| RTL-GAP-10 | none | ? | sample-rate and IF/resampler programming | GAP | needs CAP | agent, 2026-10-19 | — |
| RTL-GAP-11 | none | ? | each board's tuner and demodulator crystal frequency | GAP | needs EXP (board inspection) or DESC/CAP | agent, 2026-10-19 | — |
| RTL-GAP-12 | `control::ControlPort` (its shape) | assumed | that the dongle is controlled by vendor control transfers on endpoint 0 and streams samples on one bulk IN endpoint | GAP | needs DESC + CAP | agent, 2026-10-20 | — |
| RTL-GAP-13 | rtlsdr-sim `Wiring` (its shape: `gate` is optional) | assumed | that the host reaches the tuner's I2C port through the demodulator, behind a gate the demodulator controls | GAP | needs CAP | agent, 2026-10-20 | — |

## Not ledgered

Synthetic test values (`tests/common/synthetic.rs`, `tests/fixtures/synthetic-session.trace`,
`rtlsdr-sim`'s format tests) are invented for the harness, labelled SYNTHETIC, and never
cited here. `bmRequestType`'s direction and type bits in the tests' adapters are from USB 2.0
§9.3.1, Table 9-2; the usb port's own bit handling is `usb`'s.
