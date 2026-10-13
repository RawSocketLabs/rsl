// libusdr low-level transport that forwards every operation to rsl-pe-sync-sim (Rust).
//
// libusdr's plugin list is fixed at build time (lowlevel/usdr_lowlevel.c), so this file
// supplies the three registration functions it calls: the USB entries never match a
// device, and the PCIe entry returns the simulated PE_SYNC board.
//
// Pattern copied from the XSDR oracle's shim (devices/sdr/xsdr/oracle/shim/sim_plugin.c).

#include <errno.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "usdr_lowlevel.h"
#include "device/device.h"
#include "device/device_ids.h"
#include "hw/lmx1204/lmx1204.h"
#include "hw/lmx1214/lmx1214.h"
#include "hw/lmx2820/lmx2820.h"

// Implemented in src/lib.rs. Returns 0 or a negative errno.
int rsl_pe_sync_oracle_ls_op(unsigned op, unsigned addr, size_t insz, void* pin, size_t outsz,
                             const void* pout);
void rsl_pe_sync_oracle_sleep_us(unsigned long long us);

// Replaces libc's usleep so libusdr's delays advance the board's virtual clock instead.
int usleep(useconds_t us)
{
    rsl_pe_sync_oracle_sleep_us(us);
    return 0;
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
    // The USB VID/PID map gives PE_SYNC no SDR type (lowlevel/libusb_vidpid_map.h).
    case LLGO_DEVICE_SDR_TYPE: *pout = (const char*)SDR_NONE; return 0;
    }
    return -EINVAL;
}

static int sim_ls_op(lldev_t dev, subdev_t subdev, unsigned op, lsopaddr_t addr,
                     size_t meminsz, void* pin, size_t memoutsz, const void* pout)
{
    (void)dev;
    (void)subdev;
    return rsl_pe_sync_oracle_ls_op(op, addr, meminsz, pin, memoutsz, pout);
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

static int sim_create(unsigned pcount, const char** devparam, const char** devval,
                      lldev_t* odev, unsigned vidpid, void* webops, uintptr_t param)
{
    (void)vidpid; (void)webops; (void)param;
    struct sim_dev* d = calloc(1, sizeof(*d));
    if (d == NULL) {
        return -ENOMEM;
    }
    d->ll.ops = &s_sim_ops;
    // The device is chosen by this 16-byte ID, which a real transport takes from the board
    // (USB: VID 0x3727 / PID 0xE001 in lowlevel/libusb_vidpid_map.h). PE_SYNC has no
    // variants: nothing after this reads a hardware ID.
    d->id = PE_SYNC_DEVICE_ID_C;

    // Same order as pcie_uram_plugin_create; like it, a failed init frees only the handle
    // (pe_sync's initialize powers the board off itself on failure).
    int err = usdr_device_create(&d->ll, d->id);
    err = err ? err : d->ll.pdev->initialize(d->ll.pdev, pcount, devparam, devval);
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

// The clock chips' drivers called directly, for the chip parity tests: each runs libusdr's
// chip calls on a zeroed chip state (as pe_sync's malloc'd state is zeroed by each
// *_create) whose SPI reaches the installed board through this plugin's ls_op, and returns
// the result. The device ID stays zero: nothing here reads it.

static void sim_attach(struct sim_dev* d)
{
    memset(d, 0, sizeof(*d));
    d->ll.ops = &s_sim_ops;
}

// lmx2820_tune on SPI bus `bus`, SYSREF off (zeroed): solver, registers, lock wait.
int rsl_pe_sync_oracle_lmx2820_tune(unsigned bus, uint64_t osc_in, unsigned mash_order,
                                    unsigned force_mult, uint64_t rfouta, uint64_t rfoutb)
{
    struct sim_dev d;
    sim_attach(&d);
    lmx2820_state_t st;
    memset(&st, 0, sizeof(st));
    st.dev = &d.ll;
    st.lsaddr = bus;
    return lmx2820_tune(&st, osc_in, mash_order, force_mult, rfouta, rfoutb);
}

// lmx1214_solver's arguments; `outputs` holds out_en[n] in bit n.
struct rsl_lmx1214_request {
    uint64_t in, out;
    unsigned outputs, aux_enable, aux_fmt;
    double aux_freq;
};

// lmx1214_solver on SPI bus 5, writing its registers, without precision mode.
int rsl_pe_sync_oracle_lmx1214_solver(const struct rsl_lmx1214_request* r)
{
    struct sim_dev d;
    sim_attach(&d);
    lmx1214_state_t st;
    memset(&st, 0, sizeof(st));
    st.dev = &d.ll;
    st.lsaddr = 5;
    bool en[LMX1214_OUT_CNT];
    for (unsigned i = 0; i < LMX1214_OUT_CNT; i++) {
        en[i] = (r->outputs >> i) & 1;
    }
    lmx1214_auxclkout_cfg_t aux;
    aux.freq = r->aux_freq;
    aux.enable = r->aux_enable != 0;
    aux.fmt = (uint8_t)r->aux_fmt;
    return lmx1214_solver(&st, r->in, r->out, en, &aux, false, false);
}

// The lmx1204_state_t fields a board sets; the masks hold channel n (4: logic) in bit n.
struct rsl_lmx1204_request {
    uint64_t clkin, sysrefreq;
    unsigned filter_mode, sysref_mode, sysref_en, ch_en, clkout_en, sysrefout_en;
    unsigned logiclkout_fmt, logisysrefout_fmt;
    double clkout, sysrefout, logiclkout;
};

// lmx1204_solver on SPI bus 4, writing its registers, without precision mode; then, if it
// succeeded, lmx1204_wait_pll_lock(.., 1000000) as pe_sync waits. The state starts as
// lmx1204_create leaves it: zeroed but for every output's SYSREF delay (phase QCLK0 = 1,
// I = 7, Q = 0x7f - 7).
int rsl_pe_sync_oracle_lmx1204_solver(const struct rsl_lmx1204_request* r)
{
    struct sim_dev d;
    sim_attach(&d);
    lmx1204_state_t st;
    memset(&st, 0, sizeof(st));
    st.dev = &d.ll;
    st.lsaddr = 4;
    for (unsigned i = 0; i <= LMX1204_CH_LOGIC; i++) {
        lmx1204_sysrefout_channel_delay_t* delay = &st.sysref_indiv_ch_delay[i];
        delay->phase = 1;
        delay->i = 7;
        delay->q = 0x7f - delay->i;
        st.ch_en[i] = (r->ch_en >> i) & 1;
        st.clkout_en[i] = (r->clkout_en >> i) & 1;
        st.sysrefout_en[i] = (r->sysrefout_en >> i) & 1;
    }
    st.clkin = r->clkin;
    st.sysrefreq = r->sysrefreq;
    st.filter_mode = r->filter_mode != 0;
    st.sysref_mode = (uint8_t)r->sysref_mode;
    st.sysref_en = r->sysref_en != 0;
    st.clkout = r->clkout;
    st.sysrefout = r->sysrefout;
    st.logiclkout = r->logiclkout;
    st.logiclkout_fmt = (uint8_t)r->logiclkout_fmt;
    st.logisysrefout_fmt = (uint8_t)r->logisysrefout_fmt;
    int res = lmx1204_solver(&st, false, false);
    return res ? res : lmx1204_wait_pll_lock(&st, 1000000);
}

static const struct lowlevel_plugin s_sim_plugin = { sim_info_str, sim_discovery, sim_create };
static const struct lowlevel_plugin s_no_device = { sim_info_str, sim_discovery, no_device_create };

const struct lowlevel_plugin* usb_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* usbft601_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* pcie_uram_register(void) { return &s_sim_plugin; }
