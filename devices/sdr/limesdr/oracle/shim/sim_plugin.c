// libusdr low-level transport that stands in for the FT601 USB transport and forwards
// every control operation, as LMS64C frames, to rsl-limesdr-sim (Rust).
//
// libusdr's plugin list is fixed at build time (lowlevel/usdr_lowlevel.c), so this file
// supplies the three registration functions it calls: the URAM USB and PCIe entries never
// match a device, and the FT601 entry returns the simulated LimeSDR-mini (u3_limesdr).
//
// The real FT601 transport (lowlevel/usb_ft601/usb_ft601_generic.c) is not vendored, so
// how it maps libusdr's ls_op classes onto LMS64C commands is not in the pinned source.
// The mapping below is this shim's assumption (see devices/sdr/limesdr/AGENTS.md); the
// frames themselves are built and parsed by libusdr's own ipblks/lms64c_proto.c.
//
// Pattern copied from the XSDR oracle's shim (devices/sdr/xsdr/oracle/shim/sim_plugin.c).

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "usdr_lowlevel.h"
#include "device/device.h"
#include "device/device_ids.h"
#include "ipblks/lms64c_cmds.h"
#include "ipblks/lms64c_proto.h"

_Static_assert(sizeof(proto_lms64c_t) == LMS64C_PKT_LENGTH, "an LMS64C frame is 64 bytes");
// lms64c_parse_packet ("TODO: Endianness") and this shim copy 16/32-bit values natively.
_Static_assert(__BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__, "the oracle assumes a little-endian host");

// Implemented in src/lib.rs. Returns 0 or a negative errno.
int rsl_limesdr_oracle_lms64c(const uint8_t* request, uint8_t* reply);
void rsl_limesdr_oracle_sleep_us(unsigned long long us);

// Replaces libc's usleep so libusdr's delays advance the board's virtual clock instead.
int usleep(useconds_t us)
{
    rsl_limesdr_oracle_sleep_us(us);
    return 0;
}

// Register records one frame holds: 56 data bytes over 4-byte records (writes, and the
// replies to reads).
enum { MAX_REGS = LMS64C_DATA_LENGTH / 4 };

// One LMS64C exchange: builds a single request frame for `cmd` from `len` bytes of `data`,
// hands it to the board and, when `reply_len` is non-zero, parses the reply's payload into
// `reply`. A reply status other than STATUS_COMPLETED_CMD is -EIO.
static int sim_exchange(uint8_t cmd, const void* data, unsigned len, void* reply,
                        unsigned reply_len)
{
    // lms64c_fill_packet stores 32-bit words into the payload (offset 8): keep it aligned.
    _Alignas(4) proto_lms64c_t req, rep;
    if (lms64c_fill_packet(cmd, 0, 0, data, len, &req, 1) != 1) {
        return -EINVAL;
    }
    int res = rsl_limesdr_oracle_lms64c((const uint8_t*)&req, (uint8_t*)&rep);
    if (res) {
        return res;
    }
    if (rep.status != STATUS_COMPLETED_CMD) {
        return -EIO;
    }
    if (reply_len && lms64c_parse_packet(cmd, &rep, 1, reply, reply_len)) {
        return -EIO;
    }
    return 0;
}

// USDR_LSOP_HWREG: gateware registers are 16 bits wide (limesdr_ctrl.c uses reg_wr16 and
// reg_rd16). Assumption: a wider access spans consecutive registers, lowest first, so
// reg_rd32(0x0072) reads 0x0072 as bits 15:0 and 0x0073 as bits 31:16. Writes go out as
// one CMD_BRDSPI_WR of (address << 16 | value) records, then reads as one CMD_BRDSPI_RD.
static int sim_hwreg(unsigned addr, size_t insz, void* pin, size_t outsz, const void* pout)
{
    if (insz % 2 || outsz % 2 || insz / 2 > MAX_REGS || outsz / 2 > MAX_REGS) {
        return -EINVAL;
    }
    int res = 0;
    if (outsz) {
        uint32_t records[MAX_REGS];
        uint16_t values[MAX_REGS];
        memcpy(values, pout, outsz);
        for (unsigned i = 0; i < outsz / 2; i++) {
            records[i] = (uint32_t)((addr + i) & 0xffff) << 16 | values[i];
        }
        res = sim_exchange(CMD_BRDSPI_WR, records, (unsigned)(outsz * 2), NULL, 0);
    }
    if (!res && insz) {
        uint16_t addrs[MAX_REGS];
        for (unsigned i = 0; i < insz / 2; i++) {
            addrs[i] = (uint16_t)(addr + i);
        }
        res = sim_exchange(CMD_BRDSPI_RD, addrs, (unsigned)insz, pin, (unsigned)insz);
    }
    return res;
}

// USDR_LSOP_SPI: one 32-bit LMS7002M word on bus 0, the board's only SPI device
// (limesdr_ctrl.c: LMS7_BUS_ADDR). Assumption: a word with the write flag (bit 31) goes out
// unchanged as a CMD_LMS7002_WR record; any other word is a read of the address in bits
// 30:16, sent as CMD_LMS7002_RD, whose 16-bit value comes back zero-extended (limesdr_init
// compares the whole word with 0x3841). A write's readback is 0.
static int sim_spi(unsigned addr, size_t insz, void* pin, size_t outsz, const void* pout)
{
    if ((addr & 0xff) != 0 || outsz != 4 || (insz != 0 && insz != 4)) {
        return -EINVAL;
    }
    uint32_t word, read = 0;
    memcpy(&word, pout, sizeof(word));
    int res;
    if (word & 0x80000000u) {
        res = sim_exchange(CMD_LMS7002_WR, &word, sizeof(word), NULL, 0);
    } else {
        uint16_t reg = (uint16_t)(word >> 16), value = 0;
        res = sim_exchange(CMD_LMS7002_RD, &reg, sizeof(reg), &value, sizeof(value));
        read = value;
    }
    if (!res && insz) {
        memcpy(pin, &read, sizeof(read));
    }
    return res;
}

// USDR_LSOP_CUSTOM_CMD: limesdr_init's "LMS7 RESET" (address 0, no data). Assumption: the
// transport pulses the reset pin with CMD_LMS7002_RST, LMS_RST_PULSE (lms64c_proto.h
// defines the levels; nothing vendored uses them).
static int sim_custom(unsigned addr)
{
    if (addr != 0) {
        return -EOPNOTSUPP;
    }
    const uint8_t level = LMS_RST_PULSE;
    return sim_exchange(CMD_LMS7002_RST, &level, sizeof(level), NULL, 0);
}

// Gateware identity through libusdr's own builder (lms64c_fill_get_fpga_info: registers
// 0x0000-0x0003), for rsl-limesdr-oracle's tests. Nothing vendored calls the builder; the
// real transport presumably does at open.
int rsl_limesdr_shim_fpga_info(uint16_t out[4])
{
    _Alignas(4) proto_lms64c_t req, rep;
    if (lms64c_fill_get_fpga_info(&req) != 1) {
        return -EINVAL;
    }
    int res = rsl_limesdr_oracle_lms64c((const uint8_t*)&req, (uint8_t*)&rep);
    if (res) {
        return res;
    }
    if (rep.status != STATUS_COMPLETED_CMD) {
        return -EIO;
    }
    return lms64c_parse_packet(CMD_BRDSPI_RD, &rep, 1, (uint8_t*)out, 4 * sizeof(uint16_t))
               ? -EIO
               : 0;
}

struct sim_dev {
    lowlevel_dev_t ll;  // must be first: libusdr casts lldev_t to it
    device_id_t id;
};

static int sim_generic_get(lldev_t dev, int op, const char** pout)
{
    struct sim_dev* d = (struct sim_dev*)dev;
    switch (op) {
    case LLGO_DEVICE_NAME: *pout = "sim"; return 0;
    case LLGO_DEVICE_UUID: *pout = (const char*)d->id.d; return 0;
    case LLGO_DEVICE_SDR_TYPE: *pout = (const char*)SDR_LIME; return 0;
    }
    return -EINVAL;
}

static int sim_ls_op(lldev_t dev, subdev_t subdev, unsigned op, lsopaddr_t addr,
                     size_t meminsz, void* pin, size_t memoutsz, const void* pout)
{
    (void)dev;
    (void)subdev;
    switch (op) {
    case USDR_LSOP_HWREG: return sim_hwreg(addr, meminsz, pin, memoutsz, pout);
    case USDR_LSOP_SPI: return sim_spi(addr, meminsz, pin, memoutsz, pout);
    case USDR_LSOP_CUSTOM_CMD: return sim_custom(addr);
    }
    // Not used by open or close; fail loudly rather than guess.
    return -EOPNOTSUPP;
}

static int sim_destroy(lldev_t dev)
{
    if (dev->pdev) {
        dev->pdev->destroy(dev->pdev);
    }
    free(dev);
    return 0;
}

static lowlevel_ops_t s_sim_ops = {
    .generic_get = sim_generic_get,
    .ls_op = sim_ls_op,
    .destroy = sim_destroy,
};

static const char* sim_info_str(unsigned iparam)
{
    (void)iparam;
    return "sim";
}

static int sim_discovery(unsigned pcount, const char** filterparams, const char** filtervals,
                         unsigned maxbuf, char* outbuf)
{
    (void)pcount; (void)filterparams; (void)filtervals; (void)maxbuf; (void)outbuf;
    return -ENODEV;
}

// What the last sim_create returned. lowlevel_create tries every plugin in turn and returns
// the last one's result, so when the FT601 entry fails the PCIe entry's -ENODEV is all
// usdr_dmd_create_string reports; the oracle reads the device's own errno here.
static int s_create_errno;

int rsl_limesdr_shim_create_errno(void)
{
    return s_create_errno;
}

static int sim_create(unsigned pcount, const char** devparam, const char** devval,
                      lldev_t* odev, unsigned vidpid, void* webops, uintptr_t param)
{
    (void)vidpid; (void)webops; (void)param;
    struct sim_dev* d = calloc(1, sizeof(*d));
    if (d == NULL) {
        return s_create_errno = -ENOMEM;
    }
    d->ll.ops = &s_sim_ops;
    // The family is chosen by this 16-byte ID, which the FT601 transport takes from the
    // VID/PID map (lowlevel/libusb_vidpid_map.h: 0x0403:0x601f and 0x0403:0x601e, both
    // LimeSDR-mini). Nothing on the wire identifies the board to libusdr.
    d->id = U3_LIMESDR_DEVICE_ID_C;

    // Same order as the XSDR oracle's shim; a failed init frees only the handle.
    int err = usdr_device_create(&d->ll, d->id);
    err = err ? err : d->ll.pdev->initialize(d->ll.pdev, pcount, devparam, devval);
    s_create_errno = err;
    if (err) {
        free(d);
        return err;
    }
    *odev = &d->ll;
    return 0;
}

static int no_device_create(unsigned pcount, const char** devparam, const char** devval,
                            lldev_t* odev, unsigned vidpid, void* webops, uintptr_t param)
{
    (void)pcount; (void)devparam; (void)devval; (void)odev; (void)vidpid; (void)webops; (void)param;
    return -ENODEV;
}

static const struct lowlevel_plugin s_sim_plugin = { sim_info_str, sim_discovery, sim_create };
static const struct lowlevel_plugin s_no_device = { sim_info_str, sim_discovery, no_device_create };

const struct lowlevel_plugin* usb_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* usbft601_uram_register(void) { return &s_sim_plugin; }
const struct lowlevel_plugin* pcie_uram_register(void) { return &s_no_device; }
