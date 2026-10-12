# LimeSDR family (`u3_limesdr`): LimeSDR-mini

> Inherits `../../AGENTS.md` (devices) and the workspace-root `../../../AGENTS.md`. The
> uSDR guide (`../usdr/AGENTS.md`) and ledger (`../usdr/PORTING.md`, rows "LimeSDR board /
> framing / streams" and "FT601 USB transport") govern the port programme; this file holds
> the family's own facts.

Groundwork for a pure-Rust port of libusdr's LimeSDR family: the board simulator and the
libusdr reference oracle. The driver is **not started**. Board: an FT601 USB 3 bridge,
gateware with a 16-bit register file, firmware answering LMS64C control frames, and an
LMS7002M RFIC. Nothing here is hardware-qualified.

| Path | Crate | Role |
|------|-------|------|
| `.` | `rsl-limesdr` (planned, not in tree) | The driver |
| `sim/` | `rsl-limesdr-sim` (unpublished, root workspace member) | Board model at the LMS64C seam: gateware registers with the reference-clock detector, the LMS7002M, virtual clock, trace of frames and decoded register operations |
| `oracle/` | `rsl-limesdr-oracle` (unpublished, own workspace: compiles C) | libusdr's `u3_limesdr` driving `sim` through `oracle/shim/sim_plugin.c` |
| `../chips/lms7002m/sim/` | `rsl-lms7002m-sim` (shared with `rsl-xsdr-sim`) | The LMS7002M register file; its assumptions are in `../xsdr/AGENTS.md` and the crate docs |

## Layout decision (2026-10-12)

One crate set per family, as for XSDR. There is **one vendored libusdr tree and one PIN**:
`oracle/build.rs` compiles `../../usdr/oracle/libusdr` by relative path (its `sources.txt`
already lists `device/u3_limesdr/*`, `ipblks/lms64c_proto.c` and `stream_limesdr.c`).
Never copy it here or edit it; a PIN bump there must re-bless the golden below in the same
change. The shim copies the XSDR shim's pattern. The LMS7002M model moved to
`devices/sdr/chips/lms7002m/sim` when this family became its second consumer; the XSDR
goldens did not change.

## How libusdr selects the family

- The transport hands `usdr_device_create` a 16-byte device ID; `U3_LIMESDR_DEVICE_ID`
  (`device/device_ids.h`) selects `u3_limesdr`. USB derives it from VID:PID
  (`lowlevel/libusb_vidpid_map.h`): `0x0403:0x601f` and `0x0403:0x601e`, both "LimeSDR-mini",
  bus `usbft601`, `SDR_LIME`. The shim hardcodes the ID.
- **Nothing on the wire identifies the board.** The factory (`limesdr_device_create`) does
  no I/O; `/dm/revision` is an emulated constant (`dev_limesdr_sdr_revision_get`, "r2");
  no gateware identity register is read. The only gate is `limesdr_init`'s LMS7002M version
  word, which must be exactly 0x3841 (VER 7, REV 1, MASK 1), else `-EIO`.
- **Variants**: none distinguished. Both VID:PIDs take one path; code comments assume the
  LimeSDR-mini r2. The LMS64C device enum (`lms64c_cmds.h`) names `LIMESDRMINI` (17, MAX10)
  and `LIMESDRMINI_V2` (25, ECP5), but no vendored code reads it.

## Operation classes, LMS64C and environment

`u3_limesdr` issues generic `ls_op`s; the FT601 transport (`lowlevel/usb_ft601/`, **not
vendored**: `vendor.sh` excludes it) turns them into LMS64C frames. Open uses:

- `USDR_LSOP_HWREG`: 16-bit gateware registers (`lowlevel_reg_wr16`/`rd16`) and one 32-bit
  read at 0x0072 (`lowlevel_reg_rd32`, the detector count).
- `USDR_LSOP_SPI`: bus 0 (`LMS7_BUS_ADDR`), 32-bit LMS7002M words, write flag bit 31.
- `USDR_LSOP_CUSTOM_CMD`, address 0, no data: "LMS7 RESET" (`limesdr_ctrl.c:176`).

No DRP, URAM, I2C or GPI/GPO operation occurs. `/dm/power/en` is a no-op
(`dev_limesdr_pwren_set`) and close issues nothing (`limesdr_dtor` is a TODO): libusdr
leaves streaming, the LMS7002M and its reset pin as init left them.

LMS64C (`ipblks/lms64c_proto.{c,h}`, `lms64c_cmds.h`): 64-byte frames, header `cmd`,
`status`, `blockCount`, `periphID`, 4 reserved, then 56 data bytes. `lms64c_fill_packet`
builds requests: writes (`CMD_LMS7002_WR` 0x21, `CMD_BRDSPI_WR` 0x55) as big-endian 32-bit
records, four bytes per block; reads (`CMD_LMS7002_RD` 0x22, `CMD_BRDSPI_RD` 0x56) as
big-endian 16-bit addresses, at most 14 per frame. `lms64c_parse_packet` takes the value
half of 32-bit address-then-value reply records (`MOD_INT32_16`). Reset levels
`LMS_RST_{DEACTIVATE,ACTIVATE,PULSE}` are defined; `lms64c_fill_get_board_info`
(`CMD_GET_INFO`) and `lms64c_fill_get_fpga_info` (registers 0x0000-0x0003) exist. Nothing
vendored calls any of these: the FT601 transport does.

The oracle panics if `USDR_DEBUG` (debug socket service in `_usdr_dmd_create`) or
`USDR_BARE_DEV` (stream initialisation refuses) is set (`rsl_limesdr_oracle::FORBIDDEN_ENV`).
`USDR_LOGLEVEL` only changes logging. The FT601 transport's own variables are unknown.

## The shim's transport assumptions

The oracle shim stands in for the FT601 transport, so these are **assumptions**, not
libusdr facts; each `req`/`rep` golden line encodes them. Frames are built and parsed by
the vendored `lms64c_proto.c`.

- `HWREG` → one `CMD_BRDSPI_WR` of `(address << 16 | value)` records, then one
  `CMD_BRDSPI_RD`. A 32-bit access spans two registers, low first (`0x0072` | `0x0073` << 16).
- `SPI` → `CMD_LMS7002_WR` with the word unchanged when bit 31 is set, else
  `CMD_LMS7002_RD` of bits 30:16; the 16-bit result is zero-extended (`limesdr_init`
  compares the whole word). Peripheral 0.
- `CUSTOM_CMD` 0 → `CMD_LMS7002_RST` with `LMS_RST_PULSE`.
- A reply status other than `STATUS_COMPLETED_CMD` is `-EIO`; no USB framing, endpoints or
  FT601 configuration is modelled.

## What the sim models, and its assumptions

Each is stated again, with its evidence, where it is encoded (`sim/src/board.rs`).

- Replies echo command, block count and peripheral; reads answer one address-then-value
  record per address (consistent with `lms64c_parse_packet`). Modelled: the four register
  commands and the reset pulse. Other commands reply `STATUS_UNKNOWN_CMD`, other reset
  levels or a non-zero LMS7002M peripheral `STATUS_ERROR_CMD`, overflowing block counts
  `STATUS_MANY_BLOCKS_CMD`; a refused frame changes nothing.
- Gateware registers: plain 16-bit storage, unwritten reads 0, addresses taken whole.
  Identity values (0x0000-0x0003) are not in the source; `with_fpga_reg` sets them.
- Reference-clock detector: writing the start bit (0x0061 bit 2) completes at once, putting
  the reference's cycle count over 16 777 210 cycles of 100 MHz (the C's own constants) in
  0x0072/0x0073 and setting 0x0065 bit 2; clearing the start bit clears done. Default
  reference 40 MHz (a board fact, not in the source), which counts exactly 6 710 884.
- LMS7002M: `rsl-lms7002m-sim` with metal mask 1; the reset pulse returns it to power-on
  (nothing written). Bit 15 of an address is the write flag.
- Sleeps advance virtual time only (the shim replaces `usleep`).

## libusdr behaviour a port must reproduce or deliberately fix

- `limesdr_detect_refclk`: its `ABS` macro evaluates `clkTbl[i++]` twice and the subtraction
  is unsigned, so the nearest-entry walk skips entries and wraps. On an ideal counter only
  references measuring 40-41 MHz open; 10 MHz is taken for 30.72 MHz and refused; counts
  just below 40 MHz, 30.72 MHz or exactly 52 MHz read `clkTbl[5]` (out of bounds; the
  52 MHz test row passes only because the sim's integer count reads 51 999 998). Its poll loop tests
  `j == 100` after at most 10 iterations, so a detector that never finishes is not caught.
- `lowlevel_create` returns the last transport plugin's result, so a failed LimeSDR init
  reaches the caller as `-ENODEV`; the oracle reports the device's own errno separately
  (`OpenError::device_errno`).
- `lms64c_parse_packet` consumes 56 output bytes per reply frame while
  `lms64c_fill_packet` puts 14 addresses (28 output bytes) in each read request, and reads
  past the 56-byte payload for more than 14: reads over 14 registers misparse.

## Characterization golden

`oracle/tests/golden/limesdr_mini_session.trace`: `usdr_dmd_create_string("")`,
`/dm/power/en` = 1, `usdr_dmd_close` on a fresh board, one op per line in
`rsl_limesdr_sim::Op`'s `Display` format (`req`/`rep` frames around the register operations
each decoded to), `== open` / `== power-up` / `== close` markers and a provenance header.
**Characterization, not specification.** Open: 20 exchanges, 61 ops (detector start, one poll,
count read; LMS7 reset; version read; `lms7002m_create`'s 10 writes and version read;
stream-control read-modify-write of 0x000A). Power-up and close: empty.
`oracle/tests/identify.rs` covers device-ID-only identity, the 0x3841 gate, reset-first
ordering, the reference-clock outcomes above and the vendored gateware-info builder.
Not covered: sample rate, tuning, gain, bandwidth, streams, `stream_limesdr.c`.

## Verify

```sh
cargo fmt -p rsl-limesdr-sim -p rsl-lms7002m-sim --check
cargo clippy -p rsl-limesdr-sim -p rsl-lms7002m-sim --all-targets -- -D warnings
cargo test -p rsl-limesdr-sim -p rsl-lms7002m-sim
cargo fmt --manifest-path devices/sdr/limesdr/oracle/Cargo.toml -- --check
cargo clippy --manifest-path devices/sdr/limesdr/oracle/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path devices/sdr/limesdr/oracle/Cargo.toml
# Re-bless (PIN bump or deliberate sim change only), then review the diff:
RSL_LIMESDR_BLESS=1 cargo test --manifest-path devices/sdr/limesdr/oracle/Cargo.toml --test init_golden
```

A change to `rsl-lms7002m-sim` must also pass the XSDR oracle unchanged. CI runs the
oracle as the `limesdr-oracle` profile; a change to the vendored C selects it too. One
oracle is open at a time per process (a lock), since libusdr is process-global.

## Open items and blockers

- The FT601 transport C (`lowlevel/usb_ft601/usb_ft601_{generic,libusb}.c`) is not
  vendored, so the `ls_op` → LMS64C mapping, transport-level open traffic (board info,
  FIFO reset) and FT601 framing are unverified. Vendoring it (a PIN-subset change in
  `../usdr/oracle/vendor.sh`) turns the shim assumptions above into evidence.
- Firmware semantics (reply layout, write-flag handling, reset levels) and gateware
  identity values need LimeSuite or firmware source, neither vendored.
- LMS7002M reset values and channel banking (see `../xsdr/AGENTS.md`) before rate/tune.
- Streams: `stream_limesdr.c`, `limesdr_setup_stream`, the 16-byte data header.
