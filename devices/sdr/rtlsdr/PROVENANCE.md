# rtlsdr — provenance ledger

Every register address, value, field, request code, identifier and sequence in `rtlsdr`
and `rtlsdr-sim` has a row here, and every such constant in code names its row with a
`prov: <ID>` doc tag. A value with no admissible source is a GAP row and, in code, a
`Pending` placeholder that refuses at runtime; nothing is guessed.

**Licensing rule (owner's, absolute).** Target licence MIT OR Apache-2.0. No GPL RTL-SDR
source or anything derived from one is read, searched or quoted, and recollection of such
sources is not evidence: librtlsdr/osmocom rtl-sdr and forks, the Linux kernel's
rtl28xxu/rtl2832/r820t/e4000/fc001x drivers, rtl-sdr-rs, pyrtlsdr internals, SDR++/gqrx.
Captures of librtlsdr's USB traffic are allowed, tagged GPLOBS, and may be a row's sole
source (owner's decision, 2026-11-09; `CAPTURES.md`).

## Classes

| Class | Meaning |
|---|---|
| DS | A vendor document, cited by document, page and table |
| DESC | A USB descriptor dump of an owner's dongle (`lsusb -v`, sysfs), cited by capture id |
| CAP | A usbmon/Wireshark capture of the vendor's Windows software, cited by capture id + sha256 |
| EXP | An experiment on the owner's hardware, cited by its recorded procedure and result |
| GPLOBS | A capture of librtlsdr's traffic (its behaviour only, never its source), cited by capture id + sha256; may stand alone since 2026-11-09 |
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
| R820-DS-01 | `r820t2::registers::I2C_ADDRESS` | `0x1A` | 7-bit I2C slave address; write byte 0x34, read byte 0x35 | DS | R820T2-RD p. 2, §1.1, Table 1-1 | agent, 2026-10-19 | GPLOBS-V5-0001 #91, #92: the V5's tuner answers at `0x34` = `0x1a` << 1 (RTL-OBS-06); the V4's R828D still `R820-GAP-11` |
| R820-DS-02 | `registers::REGISTER_COUNT`; sim `tuner::REGISTER_COUNT` | 32 | registers R0-R31 | DS | R820T2-RD p. 5, §1.2 "Total 32 registers"; T1-2 rows 0x00-0x1F | agent, 2026-10-19 | open conflict: the V5's open (RTL-OBS-09) page-writes R26 to R32 (GPLOBS-V5-0001 #108) and R33, R34 (#109), past the document's 32; which tuner the V5 carries, and how many registers it has, is not evidenced |
| R820-DS-03 | `Registers::page_write`; sim `R820t2::write` | first byte = register address, then consecutive registers | I2C write mode (page write) | DS | R820T2-RD p. 3, Figure 1-1 | agent, 2026-10-19 | none yet |
| R820-DS-04 | `Registers::from_read`; sim `R820t2::read` | read from R0 on; each byte LSB first, so an MSB-first master sees it bit-reversed | I2C read mode | DS | R820T2-RD p. 4, "Reading data transmission begins from core register 0 … transmitted from LSB to MSB" | agent, 2026-10-19 | Confirmed on the wire. The bit reversal was this crate's reading of "LSB to MSB" against I2C's normal MSB-first byte order (NXP UM10204, *I2C-bus specification and user manual*, §3.1.5 "Byte format"; not yet held under `references/`). The driver (`Registers::from_read`) and the sim (`R820t2::read`) share this one reading, so their cross-check cannot catch it; GPLOBS-V5-0001 #92 reads `0x69` for R0, confirming it |
| R820-DS-05 | `registers::CHIP_ID`; sim `tuner::CHIP_ID` | `0x96` | R0's fixed content, the read-mode reference check point | DS | R820T2-RD p. 4 (text), T1-2 row R0 (B7..B0 = 1 0 0 1 0 1 1 0) | agent, 2026-10-19 | GPLOBS-V5-0001 #92 (`0x69` on the wire) |
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
| R820-DS-21 | `field::{SEL_DIV, REF_DIV2, CAPX}`, `setting::{MixerDivider, RefDivider, XtalCap}` | R16[7:5]: 000 /2, 001 /4, 010 /8, 011 /1; [4]: 0 fref = xtal, 1 fref = xtal/2; [1:0]: 0/10/20/30 pF | VCO-to-mixer divider, reference divider, crystal load | DS | R820T2-RD T1-3 p. 9 (named `SEL_DIV[3:0]` for bits [7:5], `SEL_DIV[2:0]` in T1-2; bitmap used) | agent, 2026-10-19 | none; codes 100-111 `R820-GAP-03`; conflict: on the V5 (RTL-OBS-15) `SEL_DIV` 011 divides by 16, and 100, 101 by 32, 64; the driver takes the captures' reading, the `setting::MixerDivider` decoding stays the document's |
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
| R820-GAP-01 | none | ? | how `Ndiv` relates f_vco to fref (not stated) | GAP | needs a second document or CAP + EXP | agent, 2026-10-19 | the V5's captures fit VCO = Ndiv · 28.8 MHz (RTL-OBS-15) |
| R820-GAP-02 | none | ? | VCO range, and so `SEL_DIV` choice per LO frequency | GAP | needs document or EXP | agent, 2026-10-19 | librtlsdr's choice on the V5: the smallest divider with VCO ≥ 1.77 GHz (RTL-OBS-15); the chip's own range is still not evidenced |
| R820-GAP-03 | `MixerDivider::from_bits` → `None` | ? | `SEL_DIV` codes 100-111 | GAP | not in R820T2-RD | agent, 2026-10-19 | 100 and 101 are /32 and /64 on the V5 (RTL-OBS-15) |
| R820-GAP-04 | none | ? | LNA_GAIN and MIX_GAIN steps in dB | GAP | needs document or EXP | agent, 2026-10-19 | — |
| R820-GAP-05 | none | ? | FILT_CODE, FILT_BW, HPF corners in Hz | GAP | needs document or EXP | agent, 2026-10-19 | — |
| R820-GAP-06 | `RfMux`/`PllAutoClock`/`RfFilterBand::from_bits` → `None` | ? | RFMUX 10/11, PLL_AUTO_CLK 11, RFFILT 11; RFFILT band edges | GAP | not in R820T2-RD | agent, 2026-10-19 | — |
| R820-GAP-07 | none | ? | TF_NCH/TF_LP settings per RF frequency | GAP | needs CAP or EXP | agent, 2026-10-19 | the V5's table is RTL-OBS-17 |
| R820-GAP-08 | `Registers::page_write` refuses unread registers (`RegisterError::NotRead`); `Registers::default` knows none; the sim starts zero but R0 | ? | power-on defaults; whether T1-2's fixed 0/1 bits are required values or defaults; init values | GAP | needs CAP (vendor software's init writes) | agent, 2026-10-19 | the V5's open writes R5 on (`sequence::OPEN`, RTL-OBS-09) without reading them first; the values' meaning per field is still open |
| R820-GAP-09 | none | ? | VCO_INDICATOR / RF_INDICATOR encodings (PLL lock) | GAP | needs document or EXP | agent, 2026-10-19 | R2 bit 6 reports lock on the V5 (RTL-OBS-16); the other bits' encodings remain |
| R820-GAP-10 | none | ? | image (IMR) calibration procedure | GAP | needs CAP or EXP | agent, 2026-10-19 | — |
| R820-GAP-11 | none | ? | the R828D (Blog V4): I2C address, identity byte, register differences | GAP | no datasheet found (`SOURCES.md`); needs CAP of the V4 | agent, 2026-10-19 | — |

## RTL2832U and dongle rows

`DESC-V4-0001` is the Blog V4's descriptor dump (`lsusb -v` and sysfs, 2026-11-09; files and
SHA-256s in `references/rtlsdr-captures/SOURCES.md`, outside the repo). Its device strings:
manufacturer `RTLSDRBlog`, product `Blog V4`. `DESC-V5-0001` is the owner's "V5", a Nooelec
NESDR SMArt v5 (manufacturer `Nooelec`, product `NESDR SMArt v5`), dumped the same way the same
day. The two share `idVendor`:`idProduct`; only their strings tell them apart.

| ID | Rust item | Value | Meaning | Class | Source | Recorded | Corroboration |
|---|---|---|---|---|---|---|---|
| RTL-DESC-01 | `rtl2832u::BLOG_V4_ID` | `0bda:2838` | Blog V4 `idVendor`:`idProduct` (bcdDevice 1.00, USB 2.00 high speed, one configuration) | DESC | DESC-V4-0001 lsusb (sha256 9524c469…) device descriptor; sysfs `7-4/idVendor`, `idProduct` | agent, 2026-11-09 | sysfs agrees with lsusb |
| RTL-DESC-02 | `rtl2832u::BLOG_V4_STREAM` | interface 0, endpoint `0x81` | interface 0 (class/subclass/protocol ff/ff/ff) has one endpoint: bulk IN `0x81`, wMaxPacketSize 512; interface 1 (ff/ff/ff) has none; both are named "Bulk-In, Interface" | DESC | DESC-V4-0001 lsusb interface and endpoint descriptors; sysfs `7-4:1.0/ep_81`, `7-4:1.1` | agent, 2026-11-09 | sysfs agrees with lsusb. That interface 0's endpoint carries the samples is from its being the only data endpoint, not yet a capture |
| RTL-DESC-03 | `rtl2832u::V5_ID` | `0bda:2838` | NESDR SMArt v5 `idVendor`:`idProduct` (bcdDevice 1.00, USB 2.00 high speed, one configuration) | DESC | DESC-V5-0001 lsusb (sha256 acef8c33…) device descriptor; sysfs `7-4/idVendor`, `idProduct` | agent, 2026-11-09 | sysfs agrees with lsusb; equal to `RTL-DESC-01` |
| RTL-DESC-04 | `rtl2832u::V5_STREAM` | interface 0, endpoint `0x81` | one interface (ff/ff/ff) with one endpoint: bulk IN `0x81`, wMaxPacketSize 512; unlike the V4, no second interface | DESC | DESC-V5-0001 lsusb interface and endpoint descriptors; sysfs `7-4:1.0/ep_81` | agent, 2026-11-09 | sysfs agrees with lsusb. As `RTL-DESC-02`, that it carries the samples is not yet a capture |

## RTL2832U observed rows

From librtlsdr's traffic on the NESDR SMArt v5 (class GPLOBS, sole source by the owner's
decision of 2026-11-09): `GPLOBS-V5-0001` (`rtl_test -t`, sha256 24ec1126…, 198 vendor
transfers), `GPLOBS-V5-0002` (`rtl_sdr` 100 MHz, 2.048 MS/s, auto gain, sha256 8c8ac8f4…,
208) and `GPLOBS-V5-0003` (`rtl_sdr` 433.92 MHz, 1.024 MS/s, 20.7 dB, sha256 f7697563…,
229); files and procedures in `references/rtlsdr-captures/SOURCES.md`. `GPLOBS-V5-0004`
(`rtl_eeprom`, read only, sha256 b66beca7…, 414) bounds the open. Transfer numbers are the
vendor transfers' order in each capture, as in its trace under `tests/fixtures/`
(`gplobs-v5-000N.trace`). Rows RTL-OBS-01 to -08 hold in all four captures without
exception (`tests/observed.rs` checks 01 to 07); RTL-OBS-10 holds for the tools that do not
stream (0001, 0004). Rows 11 to 18 are fitted to the sweep (`rtl_sdr` sessions
GPLOBS-V5-0005 to -0059, one setting varied each) and the bisection GPLOBS-V5-0060 (714
sessions); `tests/fixtures/gplobs-v5-sweep-state.txt` holds every session's registers at
the stream start, and `tests/v5.rs` holds the plans to all of them (RTL-OBS-14's writes, which the table
does not record, to the order 0016 shows). The names (paged block, block, gate) are this crate's own.

| ID | Rust item | Value | Meaning | Class | Source | Recorded | Corroboration |
|---|---|---|---|---|---|---|---|
| RTL-OBS-01 | `rtl2832u::REQUEST_TYPE`, `rtl2832u::VENDOR_REQUEST` | `0x40` out / `0xc0` in; `bRequest` `0x00` | every register and I2C access is a vendor control transfer to the device with request 0 | GPLOBS | GPLOBS-V5-0001 to -0004, all 1049 vendor transfers | agent, 2026-11-09 | USB 2.0 Table 9-2 reads `0x40`/`0xc0` as vendor, device, out/in |
| RTL-OBS-02 | `rtl2832u::WRITE`, `Location::write_index` | `wIndex` bit 4 | set on every write, clear on every read of the same place (I2C: `0x0610` / `0x0600`; paged page 1 written at `0x0011`, page `0x0a` read at `0x000a`) | GPLOBS | as RTL-OBS-01 | agent, 2026-11-09 | — |
| RTL-OBS-03 | `rtl2832u::PAGED_VALUE`, `rtl2832u::Page`, `Location::paged` | `wValue` = register << 8, plus `0x20`; `wIndex` low nibble = page | the paged block (`wIndex` high byte 0): pages 0, 1 written, page `0x0a` read; multi-byte writes carry consecutive bytes (page 1 register `0x9f`, 2 bytes, GPLOBS-V5-0001 #172) | GPLOBS | GPLOBS-V5-0001 #7 on; 247 paged writes over the four | agent, 2026-11-09 | — |
| RTL-OBS-04 | `rtl2832u::POST_WRITE_READ` | page `0x0a`, register `0x01`, 1 byte | read right after every paged-block write (247 of 247, the two that stall in `rtl_sdr`'s teardown included), always answering `0x09`; its purpose is not known, so the driver repeats it, after a stalled write too | GPLOBS | GPLOBS-V5-0001 #8 on | agent, 2026-11-09 | — |
| RTL-OBS-05 | `rtl2832u::Block`, `Location::block` | `wValue` = 16-bit address; `wIndex` = block << 8 | blocks 1 (`0x2000`, `0x2148`, `0x2158`) and 2 (`0x3000`, `0x300b`) are written by address; no read of either is seen, so their read encoding is the I2C block's by inference | GPLOBS | GPLOBS-V5-0001 #1 to #6 | agent, 2026-11-09 | — |
| RTL-OBS-06 | `rtl2832u::I2C_BLOCK`, `Location::i2c` | block 6; `wValue` = 7-bit address << 1 | the tuner's I2C port: a write's data is a register then its values (R820-DS-03), a read returns n bytes from R0 on after a 1-byte write of register 0; the tuner answers at `0x34` (`0x1a`, R820-DS-01); addresses with no device stall (`0xc8`, `0xc6` before it); `rtl_eeprom` goes on to read `0xa0` (7-bit `0x50`, GPLOBS-V5-0004 #143 on) | GPLOBS | GPLOBS-V5-0001 #87 to #92 | agent, 2026-11-09 | R0 reads `0x69` on the wire, R820-DS-04/05's `0x96` LSB first |
| RTL-OBS-07 | `rtl2832u::GATE`, `GATE_OPEN`, `GATE_CLOSED` | page 1, register `0x01`: `0x18` / `0x10` | written `0x18` before the tuner's traffic and `0x10` after (and `0x14` twice per session, bit 2's meaning unknown); every one of the 248 completed transfers to `0x34` happens while the last completed write there had bit 3 set (two more, and two gate writes, stall in `rtl_sdr`'s teardown) | GPLOBS | GPLOBS-V5-0001 #85 (open), #141 (close) | agent, 2026-11-09 | — |
| RTL-OBS-08 | `control::ControlPort` (its shape) | — | the dongle is controlled by vendor control transfers on endpoint 0 and streams on bulk IN `0x81` (`RTL-DESC-02`, `-04`); `rtl_sdr` reads 262144-byte bulk transfers there | GPLOBS | GPLOBS-V5-0002, -0003 bulk URBs | agent, 2026-11-09 | DESC-V4-0001, DESC-V5-0001 |
| RTL-OBS-09 | `sequence::OPEN`, `Device::open` | 142 transfers | the V5's open: addressed-block and paged writes, the gate opened, two addresses probed (both stall; the pcap shows the probe writes carry `0x02` and `0x00` and each read asks 1 byte, which a trace does not record for a stalled transfer), R0 read for the tuner's identity, paged writes, the tuner's registers written and read, the gate closed; what each step does is not known | GPLOBS | GPLOBS-V5-0001 #1 to #142 | agent, 2026-11-09 | the host sends the same 142 transfers first in GPLOBS-V5-0002, -0003 and -0004 (`rtl_eeprom`, which goes on to read I2C `0x50` at #143), whatever the tuner's status reads (#115 answers `…07` and `…67`) |
| RTL-OBS-10 | `sequence::CLOSE`, `Device::close` | 15 transfers | the close of the tools that do not stream: the gate opened, ten tuner register writes, the gate closed, block 2 `0x3000` written `0x20`; `rtl_sdr`'s close (0002, 0003) differs, writing block 1 `0x2148` and stalling on the gate and the tuner, and is not ported | GPLOBS | GPLOBS-V5-0001 #184 to #198 | agent, 2026-11-09 | the last 15 transfers of GPLOBS-V5-0004 are the same |
| RTL-OBS-11 | `v5::XTAL_HZ` | 28 800 000 Hz | the V5's crystal, for the demodulator and the tuner alike | GPLOBS | fit over the sweep GPLOBS-V5-0005 to -0059 and bisection GPLOBS-V5-0060 (714 runs): the resampler ratio (RTL-OBS-12) and the PLL (RTL-OBS-15) reproduce every session's bytes exactly with it | agent, 2026-11-09 | two independent fits agree; `tests/v5.rs` |
| RTL-OBS-12 | `v5::{RATE_RANGES, RATIO_REGISTERS}`, `RatePlan::ratio` | evidenced rates 225001-300000 and 900001-3200000 S/s; ratio `floor(28.8 MHz · 2^22 / rate) & 0x0ffffffc`, big-endian, page 1 `0x9f` (2 bytes) then `0xa1` (2 bytes) | the resampler ratio; each range's ends were tried and accepted; the rates tried between (300001 to 600000) got no ratio write (librtlsdr refused them and re-initialised the tuner); 600001-900000, below 225001 and above 3.2 MS/s were not tried; the driver refuses all of these | GPLOBS | sweep GPLOBS-V5-0005 to -0059 and bisection GPLOBS-V5-0060 (714 runs); the mask from 2.1 MS/s (`0x036db6d8`) and 250 kS/s (bit 28 dropped) | agent, 2026-11-09 | `tests/v5.rs` over all sessions |
| RTL-OBS-13 | `v5::{RATE_TABLE, IF_REGISTER, FILTER_REGISTER}`, `RatePlan::{if_hz, if_bytes, filter}` | 16 rows by highest rate: IF Hz and tuner R10, R11; IF word `2^22 − floor(IF · 2^22 / 28.8 MHz)`, 22 bits big-endian at page 1 `0x19`-`0x1b` | each rate's IF (the LO sits that far above the frequency) and channel filter; the edges are exact to 1 S/s (a row covers up to and including its rate) | GPLOBS | sweep GPLOBS-V5-0005 to -0059 and bisection GPLOBS-V5-0060 (714 runs) (the rate grid and its bisection) | agent, 2026-11-09 | the LO offset fitted from the PLL equals the IF the word encodes, in every session |
| RTL-OBS-14 | `v5::{AFTER_RATIO, AFTER_RATIO_GATE}` | page 1 `0x3f`, `0x3e` written `0x00`; then the gate register `0x14`, `0x10` | follows every ratio write; what it does is not known | GPLOBS | sweep GPLOBS-V5-0005 to -0059 and bisection GPLOBS-V5-0060 (714 runs), every accepted rate | agent, 2026-11-09 | — |
| RTL-OBS-15 | `v5::{VCO_MIN_KHZ, LO_MAX_KHZ, MIXER_DIVIDERS, R16_LOW, R17_TO_R19, PLL_REGISTER}`, `TunePlan::pll` | LO = frequency + IF; `SEL_DIV` code c (divider 2^(c+1), c 0-5) the smallest with round(LO/1 kHz) · divider ≥ 1 770 000; N = round(LO · divider / (2 · 28.8 MHz) · 2^16) / 2^16 into R20-R22 (R820-DS-24, -25); R16 = c << 5, plus `0x04`; R17-R19 `bb 80 31` | the PLL write. The tuning range follows: 27 657 ≤ round(LO / 1 kHz) < 1 770 000. Below it librtlsdr wrote code 000 and a divider for another LO; at and above it, no PLL write | GPLOBS | sweep GPLOBS-V5-0005 to -0059 and bisection GPLOBS-V5-0060 (714 runs) (659 in-range sessions at 2.048 MS/s plus every rate) | agent, 2026-11-09 | conflicts R820-DS-21 (011 is /16 here, not /1) and settles R820-GAP-03 for 100, 101 (/32, /64) on this tuner; gives R820-GAP-01 (VCO = Ndiv · 28.8 MHz) and the lower half of R820-GAP-02 as librtlsdr's choice |
| RTL-OBS-16 | `v5::{LOCK_BIT, R18_RETRY, RETRY_REGISTER, PLL_READS, R26_CLOCK_AFTER}`, `Device::tune`'s order | gate open, R23, R26 (bits 3:2 clear), R27, read R0-R4, R16-R22, read R0-R2; if R2 bit 6 is clear, R18 = `0x60` and read again; then R26 with bits 3:2 = `10`, gate closed | the tune's traffic; each read is preceded by a write of register `0x00` (RTL-OBS-06). Bit 6 set in every session that went on to the R26 write, clear in every one that retried; the retry never locked here (librtlsdr's first, stale tune; 2.2 GHz), so the driver then refuses | GPLOBS | GPLOBS-V5-0016 #184 to #196 (`tests/fixtures/gplobs-v5-0016-tune.trace`, replayed); sweep GPLOBS-V5-0005 to -0059 and bisection GPLOBS-V5-0060 (714 runs) | agent, 2026-11-09 | narrows R820-GAP-09 to this one bit |
| RTL-OBS-17 | `v5::{R23_BANDS, R26_BANDS, R27_BANDS, BAND_REGISTERS}` | by LO (from Hz, inclusive): R23 `0x38`, `0x30` at 75 MHz; R26 `0x22`, `0x61` at 310, `0x60` at 588; R27 `0xdf`, `0xbe` 50, `0x8b` 55, `0x7b` 60, `0x69` 65, `0x58` 70, `0x44` 75, `0x34` 90, `0x24` 110, `0x14` 140, `0x13` 180, `0x11` 250, `0x00` 280 | band registers per LO; edges bisected to 1 kHz, then probed at the edge and 500 Hz under it at 50, 75, 310 and 588 MHz, and 1 Hz under it at 50 MHz | GPLOBS | sweep GPLOBS-V5-0005 to -0059 and bisection GPLOBS-V5-0060 (714 runs) (the frequency grid, 5 MHz steps to 400 MHz, 25 MHz to 1.775 GHz, and its bisection) | agent, 2026-11-09 | settles R820-GAP-07 for the V5; a band entered and left again between two grid points would be missed |
| RTL-OBS-18 | `v5::{GAIN_LABELS_TENTH_DB, GAIN_CODES, GAIN_REGISTERS, R5_MANUAL, R7_MANUAL, R12_MANUAL, AUTO}`, `Gain` | manual: R5 `0x10` plus the LNA code, R7 `0x60` plus the mixer code, R12 `0x68`; step k's codes ((k+1)/2, k/2), step 28 (15, 14); automatic: R5 `0x03`, R7 `0x75` (the open's) and R12 `0x6b`; labels 0.0 to 49.6 dB | `rtl_sdr -g`'s 29 steps (`rtl_test`'s printed list) and its automatic gain | GPLOBS | GPLOBS-V5-0031 to -0059 (one session per step), -0060 run 714 (`-g 0.1`, step 0); labels from GPLOBS-V5-0001's tool output | agent, 2026-11-09 | the labels are librtlsdr's, not measurements (R820-GAP-04 stands) |

## RTL2832U and dongle gaps

No admissible RTL2832U document is held. These come from the owner's RTL-SDR Blog V4 (R828D)
and Nooelec NESDR SMArt v5 dongles: descriptor dumps (DESC) and captures (CAP), per `CAPTURES.md`.

| ID | Rust item | Value | Meaning | Class | Source | Recorded | Corroboration |
|---|---|---|---|---|---|---|---|
| RTL-GAP-07 | `Device::open` (refuses with this row) | ? | the open of any board but the NESDR SMArt v5, the V4 included: `Device::open` runs the V5's and refuses when a probed address answers or the tuner's R0 is not the R820T2's | GAP | needs a capture of that board | agent, 2026-10-19; narrowed 2026-11-09 | — |
| RTL-GAP-09 | none | ? | the bulk stream's sample format | GAP | needs CAP + EXP | agent, 2026-10-19 | — |
| RTL-GAP-11 | none | ? | each board's tuner and demodulator crystal frequency but the V5's (RTL-OBS-11) | GAP | needs EXP (board inspection) or DESC/CAP | agent, 2026-10-19 | — |

## Not ledgered

Synthetic test values (`tests/common/synthetic.rs`, `tests/fixtures/synthetic-*.trace`,
`rtlsdr-sim`'s format and wiring tests) are invented for the harness, labelled SYNTHETIC,
and never cited here. `bmRequestType`'s direction and type bits in the tests' adapters are
from USB 2.0 §9.3.1, Table 9-2; the usb port's own bit handling is `usb`'s. A constant that
is no device fact (the trace format's own first line, the driver's control timeout) is tagged `prov: none`.
