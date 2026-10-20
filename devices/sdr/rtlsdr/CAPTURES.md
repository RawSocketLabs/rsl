# rtlsdr — capture policy

How RTL2832U evidence is gathered from the owner's dongles (RTL-SDR Blog V4 with an
R828D, and the "V5") and cited in `PROVENANCE.md`. The licensing rule there applies.

## Before any capture or hardware test

1. Blacklist the kernel's DVB driver so it never binds the dongle:

   ```sh
   echo 'blacklist dvb_usb_rtl28xxu' | sudo tee /etc/modprobe.d/rtlsdr-blacklist.conf
   sudo modprobe -r dvb_usb_rtl28xxu   # if loaded
   ```

   Confirm with `lsmod | grep dvb_usb_rtl28xxu` (no output) after replugging. `UsbPort`
   detaches a bound driver when it claims the interface, but captures must not include the
   kernel driver's own traffic, and its source is GPL: its behaviour is not evidence.
2. Record the dongle: board label, serial, host, kernel, date.

## Descriptors (class DESC)

`lsusb -v -d VID:PID` and the sysfs attributes of the device and its interfaces
(`/sys/bus/usb/devices/<port>/…`). These give `RTL-GAP-01` to `RTL-GAP-03`.

## Traffic captures (class CAP, or GPLOBS)

- Tools: Linux `usbmon` (`modprobe usbmon`, then Wireshark or `tcpdump -i usbmonN`, saved as
  pcapng), or Wireshark with USBPcap on Windows.
- Allowed subjects:
  - **The vendor's Windows software** (class CAP): a row may rest on it alone.
  - **librtlsdr** (class GPLOBS): observing its USB traffic is allowed, but such a capture
    is corroboration only, never a row's sole source, and nothing is read from its source.
- One session per capture, with the procedure written down (what was started, tuned or
  changed, and when) so each transfer can be tied to an action.

## Storage and citation

- Capture files are **not committed** until the owner approves. Keep them outside the
  repository, for example `~/work/orgs/rsl/references/rtlsdr-captures/`, with a
  `SOURCES.md` there listing for each: capture id, file name, SHA-256, dongle, tool,
  subject (vendor software or librtlsdr), procedure, date, who.
- Capture ids: `CAP-V4-0001`, `CAP-V5-0001`, … for CAP; `GPLOBS-V4-0001`, … for GPLOBS;
  `DESC-V4-0001`, … for descriptor dumps.
- A ledger row cites the capture id, SHA-256 and the frame or transfer numbers.

## Traces for the replay harness

`rtlsdr-sim`'s `Trace` format (`sim/src/replay.rs`) holds one control transfer per line
under a header naming the capture id and its SHA-256. A trace extracted from a capture is
a derived file: it too stays outside the repository until the owner approves, and then
lands under `tests/fixtures/` with `synthetic: no`. The only trace committed now,
`tests/fixtures/synthetic-session.trace`, is SYNTHETIC: written by hand to test the
harness, and never evidence.
