// libusdr low-level transport that forwards every operation to pe-sync-sim (Rust).
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
#include <time.h>
#include <unistd.h>

#include "usdr_lowlevel.h"
#include "device/device.h"
#include "device/device_ids.h"
#include "hw/lmx1204/lmx1204.h"
#include "hw/lmx1214/lmx1214.h"
#include "hw/lmx2820/lmx2820.h"

// Implemented in src/lib.rs. Returns 0 or a negative errno.
int pe_sync_oracle_ls_op(unsigned op, unsigned addr, size_t insz, void* pin, size_t outsz,
                             const void* pout);
void pe_sync_oracle_sleep_us(unsigned long long us);
unsigned long long pe_sync_oracle_now_us(void);

// Replaces libc's usleep so libusdr's delays advance the board's virtual clock instead.
int usleep(useconds_t us)
{
    pe_sync_oracle_sleep_us(us);
    return 0;
}

// The chip drivers' clock_gettime (renamed by build.rs in the sources whose lock waits read
// the wall clock): the board's virtual clock, whatever the clock asked for, so a lock
// wait's timeout counts the virtual time its usleeps advance.
int pe_sync_oracle_clock_gettime(clockid_t clock, struct timespec* ts)
{
    (void)clock;
    unsigned long long us = pe_sync_oracle_now_us();
    ts->tv_sec = (time_t)(us / 1000000);
    ts->tv_nsec = (long)(us % 1000000) * 1000;
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
    return pe_sync_oracle_ls_op(op, addr, meminsz, pin, memoutsz, pout);
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

// The windowing steps a chip session may run after its solver, in this order (bit n of
// `windowing`): the LMX1204's SYSREF delay reload (bit 0), then *_sysref_windowing_beforesync,
// _capture and _aftersync (bits 1 to 3). The first failure ends the session.
enum { RSL_RELOAD_DELAYS = 1, RSL_BEFORE_SYNC = 2, RSL_CAPTURE = 4, RSL_AFTER_SYNC = 8 };

// One LMX2820 session's inputs: lmx2820_sysref_chain_t's fields, then which calls run.
struct rsl_lmx2820_session {
    uint64_t osc_in, rfouta, rfoutb, srout, retune_a, retune_b;
    unsigned bus, mash_order, force_mult;
    unsigned sysref_enabled, master_mode, cont_pulse, pulse_cnt, delay_ctrl, srreq_fmt;
    // lmx2820_instant_calibration_init instead of lmx2820_tune; then lmx2820_tune_instcal
    // to retune_a/retune_b; then lmx2820_sync.
    unsigned instant, retune, sync;
};

// An LMX2820 on SPI bus `bus`, its SYSREF chain set as `s` says: lmx2820_tune (solver,
// registers, lock wait) or lmx2820_instant_calibration_init, then, as asked and while each
// succeeds, lmx2820_tune_instcal and lmx2820_sync.
int pe_sync_oracle_lmx2820_session(const struct rsl_lmx2820_session* s)
{
    struct sim_dev d;
    sim_attach(&d);
    lmx2820_state_t st;
    memset(&st, 0, sizeof(st));
    st.dev = &d.ll;
    st.lsaddr = s->bus;
    lmx2820_sysref_chain_t* sr = &st.lmx2820_sysref_chain;
    sr->enabled = s->sysref_enabled != 0;
    sr->master_mode = s->master_mode != 0;
    sr->cont_pulse = s->cont_pulse != 0;
    sr->pulse_cnt = (uint8_t)s->pulse_cnt;
    sr->delay_ctrl = (uint8_t)s->delay_ctrl;
    sr->srreq_fmt = (uint8_t)s->srreq_fmt;
    sr->srout = s->srout;
    int res = s->instant
        ? lmx2820_instant_calibration_init(&st, s->osc_in, s->mash_order, s->force_mult)
        : lmx2820_tune(&st, s->osc_in, s->mash_order, s->force_mult, s->rfouta, s->rfoutb);
    res = res || !s->retune ? res : lmx2820_tune_instcal(&st, s->retune_a, s->retune_b);
    return res || !s->sync ? res : lmx2820_sync(&st);
}

// lmx1214_solver's arguments; `outputs` holds out_en[n] in bit n.
struct rsl_lmx1214_request {
    uint64_t in, out;
    unsigned outputs, aux_enable, aux_fmt, prec_mode, windowing;
    double aux_freq;
};

// lmx1214_solver on SPI bus 5, writing its registers; then, while each succeeds, the
// windowing steps `windowing` asks for.
int pe_sync_oracle_lmx1214_solver(const struct rsl_lmx1214_request* r)
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
    int res = lmx1214_solver(&st, r->in, r->out, en, &aux, r->prec_mode != 0, false);
    res = res || !(r->windowing & RSL_BEFORE_SYNC) ? res : lmx1214_sysref_windowing_beforesync(&st);
    res = res || !(r->windowing & RSL_CAPTURE) ? res : lmx1214_sysref_windowing_capture(&st);
    return res || !(r->windowing & RSL_AFTER_SYNC) ? res : lmx1214_sysref_windowing_aftersync(&st);
}

// One SYSREF output's delay, as lmx1204_init_sysrefout_ch_delay sets it.
struct rsl_lmx1204_delay {
    unsigned phase, i, q;
};

// The lmx1204_state_t fields a board sets; the masks hold channel n (4: logic) in bit n.
// `delays` are each output's before the solver, `reload` the ones the delay reload writes.
struct rsl_lmx1204_request {
    uint64_t clkin, sysrefreq;
    unsigned filter_mode, sysref_mode, sysref_en, ch_en, clkout_en, sysrefout_en;
    unsigned logiclkout_fmt, logisysrefout_fmt, prec_mode, windowing;
    double clkout, sysrefout, logiclkout;
    struct rsl_lmx1204_delay delays[LMX1204_CH_LOGIC + 1], reload[LMX1204_CH_LOGIC + 1];
};

// Sets every output's SYSREF delay, as lmx1204_init_sysrefout_ch_delay does.
static void rsl_lmx1204_delays(lmx1204_state_t* st, const struct rsl_lmx1204_delay* delays)
{
    for (unsigned ch = 0; ch <= LMX1204_CH_LOGIC; ch++) {
        lmx1204_init_sysrefout_ch_delay(st, ch, (uint8_t)delays[ch].phase,
                                        (uint8_t)delays[ch].i, (uint8_t)delays[ch].q);
    }
}

// lmx1204_solver on SPI bus 4, writing its registers; then, if it succeeded,
// lmx1204_wait_pll_lock(.., 1000000) as pe_sync waits, and, while each succeeds, the
// windowing steps `windowing` asks for. The state starts as lmx1204_create leaves it, with
// `delays` for the SYSREF outputs.
int pe_sync_oracle_lmx1204_solver(const struct rsl_lmx1204_request* r)
{
    struct sim_dev d;
    sim_attach(&d);
    lmx1204_state_t st;
    memset(&st, 0, sizeof(st));
    st.dev = &d.ll;
    st.lsaddr = 4;
    rsl_lmx1204_delays(&st, r->delays);
    for (unsigned i = 0; i <= LMX1204_CH_LOGIC; i++) {
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
    int res = lmx1204_solver(&st, r->prec_mode != 0, false);
    res = res ? res : lmx1204_wait_pll_lock(&st, 1000000);
    if (!res && (r->windowing & RSL_RELOAD_DELAYS)) {
        rsl_lmx1204_delays(&st, r->reload);
        res = lmx1204_reload_sysrefout_ch_delay(&st);
    }
    res = res || !(r->windowing & RSL_BEFORE_SYNC) ? res : lmx1204_sysref_windowing_beforesync(&st);
    res = res || !(r->windowing & RSL_CAPTURE) ? res : lmx1204_sysref_windowing_capture(&st);
    return res || !(r->windowing & RSL_AFTER_SYNC) ? res : lmx1204_sysref_windowing_aftersync(&st);
}

static const struct lowlevel_plugin s_sim_plugin = { sim_info_str, sim_discovery, sim_create };
static const struct lowlevel_plugin s_no_device = { sim_info_str, sim_discovery, no_device_create };

const struct lowlevel_plugin* usb_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* usbft601_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* pcie_uram_register(void) { return &s_sim_plugin; }
