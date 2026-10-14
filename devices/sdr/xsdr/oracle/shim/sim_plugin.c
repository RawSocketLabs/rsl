// libusdr low-level transport that forwards every operation to xsdr-sim (Rust).
//
// libusdr's plugin list is fixed at build time (lowlevel/usdr_lowlevel.c), so this file
// supplies the three registration functions it calls: the USB entries never match a
// device, and the PCIe entry returns the simulated LMS7 family (m2_lm7_1) board.
//
// Pattern copied from the uSDR oracle's shim (devices/sdr/usdr/oracle/shim/sim_plugin.c),
// trimmed to the control plane: no streams, no PCIe layout or ABI helpers.

#include <errno.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "usdr_lowlevel.h"
#include "device/device.h"
#include "device/device_ids.h"
#include "hw/lms7002m/lms7002m.h"

// Implemented in src/lib.rs. Returns 0 or a negative errno.
int xsdr_oracle_ls_op(unsigned op, unsigned addr, size_t insz, void* pin, size_t outsz,
                          const void* pout);
void xsdr_oracle_sleep_us(unsigned long long us);

// Replaces libc's usleep so libusdr's delays advance the board's virtual clock instead.
int usleep(useconds_t us)
{
    xsdr_oracle_sleep_us(us);
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
    case LLGO_DEVICE_SDR_TYPE: *pout = (const char*)SDR_XSDR; return 0;
    }
    return -EINVAL;
}

static int sim_ls_op(lldev_t dev, subdev_t subdev, unsigned op, lsopaddr_t addr,
                     size_t meminsz, void* pin, size_t memoutsz, const void* pout)
{
    (void)dev;
    (void)subdev;
    return xsdr_oracle_ls_op(op, addr, meminsz, pin, memoutsz, pout);
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
    // The family is chosen by this 16-byte ID, which a real transport takes from the board
    // (PCIe: the kernel driver's PCIE_DRIVER_GET_UUID; USB: the VID/PID map in
    // lowlevel/libusb_vidpid_map.h). The variant within the family comes later, from HWID.
    d->id = M2_LM7_1_DEVICE_ID_C;

    // Same order as pcie_uram_plugin_create; like it, a failed init frees only the handle.
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

// One call into libusdr's LMS7002M chip driver (hw/lms7002m/lms7002m.c), for
// xsdr_oracle_lms7002m_run. Kinds and arguments are xsdr_oracle::Lms7002mCall's
// (src/lib.rs); a call that reports values beside its result writes them to out.
struct lms7002m_op {
    unsigned kind, a, b, c, d;
    int out[3];
};

// lms7002m_limelight_conf_t from its flags (a: bit 0 rxsisoddr, 1 txsisoddr, 2 ds_high,
// 3 rx_ext_rd_fclk, 4 rx_lfsr, 5 rx_tx_dig_loopback, 6 rx_port) and bytes (b: rxdiv,
// txdiv, txtspdelay, txlmldelay from the low byte up).
static lms7002m_limelight_conf_t unpack_conf(unsigned a, unsigned b)
{
    lms7002m_limelight_conf_t c;
    memset(&c, 0, sizeof(c));
    c.rxsisoddr = a & 1;
    c.txsisoddr = (a >> 1) & 1;
    c.ds_high = (a >> 2) & 1;
    c.rx_ext_rd_fclk = (a >> 3) & 1;
    c.rx_lfsr = (a >> 4) & 1;
    c.rx_tx_dig_loopback = (a >> 5) & 1;
    c.rx_port = (a >> 6) & 1;
    c.rxdiv = b & 0xff;
    c.txdiv = (b >> 8) & 0xff;
    c.txtspdelay = (b >> 16) & 0xff;
    c.txlmldelay = (b >> 24) & 0xff;
    return c;
}

// lms7002m_lml_map_t from four 2-bit slots, slot 0 lowest.
static lms7002m_lml_map_t unpack_map(unsigned x)
{
    lms7002m_lml_map_t m;
    for (unsigned i = 0; i < 4; i++) {
        m.m[i] = (x >> (2 * i)) & 3;
    }
    return m;
}

// Runs ops in order on a zeroed lms7002m_state_t (as xsdr_ctor leaves it) whose SPI
// reaches the installed board through this plugin's ls_op, storing each call's result.
int xsdr_oracle_lms7002m_run(struct lms7002m_op* ops, unsigned count, int* results)
{
    struct sim_dev d;
    memset(&d, 0, sizeof(d));
    d.ll.ops = &s_sim_ops;
    lms7002m_state_t m;
    memset(&m, 0, sizeof(m));
    m.dev = &d.ll;
    for (unsigned i = 0; i < count; i++) {
        struct lms7002m_op* op = &ops[i];
        lms7002m_lpf_params_t p = { .rcc = op->a, .r = op->b, .c = op->c };
        bool done = false;
        int gain = 0;
        switch (op->kind) {
        case 0: results[i] = lms7002m_mac_set(&m, op->a); break;
        case 1: results[i] = lms7002m_rfe_path(&m, (lms7002m_rfe_path_t)op->a, (lms7002m_rfe_mode_t)op->b); break;
        case 2: results[i] = lms7002m_trf_path(&m, (lms7002m_trf_path_t)op->a, (lms7002m_trf_mode_t)op->b); break;
        case 3: results[i] = lms7002m_afe_enable(&m, op->a, op->b, op->c, op->d); break;
        case 4: results[i] = lms7002m_xxtsp_enable(&m, (lms7002m_xxtsp_t)op->a, op->b); break;
        case 5: results[i] = lms7002m_destroy(&m); break;
        case 6: results[i] = lms7002m_limelight_configure(&m, unpack_conf(op->a, op->b)); break;
        case 7: results[i] = lms7002m_limelight_reset(&m); break;
        case 8: results[i] = lms7002m_limelight_fifo_reset(&m, op->a, op->b); break;
        case 9: results[i] = lms7002m_limelight_map(&m, op->a & 1, (op->a >> 1) & 1, unpack_map(op->b), unpack_map(op->c)); break;
        case 10: results[i] = lms7002m_limelight_switch_rx_mode(&m, unpack_conf(op->a, op->b)); break;
        case 11: results[i] = lms7002m_xxtsp_int_dec(&m, (lms7002m_xxtsp_t)op->a, op->b); break;
        case 12: results[i] = lms7002m_rxtsp_dc_corr(&m, op->a, op->b); break;
        case 13: results[i] = lms7002m_xxtsp_cmix(&m, (lms7002m_xxtsp_t)op->a, (int32_t)op->b); break;
        case 14: results[i] = lms7002m_xxtsp_bst(&m, (lms7002m_xxtsp_t)op->a); break;
        case 15: results[i] = lms7002m_xxtsp_reset(&m, (lms7002m_xxtsp_t)op->a); break;
        case 16: results[i] = lms7002m_xxtsp_bst_isdone(&m, (lms7002m_xxtsp_t)op->a, &done); op->out[0] = done; break;
        case 17: results[i] = lms7002m_rbb_path(&m, (lms7002m_rbb_path_t)op->a, (lms7002m_rbb_mode_t)op->b); break;
        case 18: results[i] = lms7002m_rbb_pga(&m, (int)op->a); break;
        case 19: results[i] = lms7002m_rbb_lpf_raw(&m, p); break;
        case 20: results[i] = lms7002m_tbb_path(&m, (lms7002m_tbb_path_t)op->a, (lms7002m_tbb_mode_t)op->b); break;
        case 21: results[i] = lms7002m_tbb_lpf_raw(&m, p); break;
        case 22: results[i] = lms7002m_tbb_gain(&m, (int8_t)op->a); break;
        case 23: results[i] = lms7002m_rfe_gain(&m, (lms7002m_rfe_gain_t)op->a, (int)op->b, &gain); op->out[0] = gain; break;
        case 24: results[i] = lms7002m_trf_gain(&m, (lms7002m_trf_gain_t)op->a, (int)op->b, &gain); op->out[0] = gain; break;
        case 25: results[i] = lms7002m_cgen_tune(&m, op->a, op->b, op->c); break;
        case 26: results[i] = lms7002m_cgen_disable(&m); break;
        case 27: results[i] = lms7002m_sxx_tune(&m, (lms7002m_sxx_path_t)op->a, op->b, op->c, op->d); break;
        case 28: results[i] = lms7002m_sxx_disable(&m, (lms7002m_sxx_path_t)op->a); break;
        case 29:
            results[i] = lms7002m_rbb_lpf_def(op->a, op->b, &p);
            op->out[0] = p.rcc; op->out[1] = p.r; op->out[2] = p.c;
            break;
        case 30:
            results[i] = lms7002m_tbb_lpf_def(op->a, op->b, &p);
            op->out[0] = p.rcc; op->out[1] = p.r; op->out[2] = p.c;
            break;
        case 31: results[i] = lms7002m_create(&d.ll, 0, 0, op->a, op->b, &m); break;
        default: return -EINVAL;
        }
    }
    return 0;
}

static const struct lowlevel_plugin s_sim_plugin = { sim_info_str, sim_discovery, sim_create };
static const struct lowlevel_plugin s_no_device = { sim_info_str, sim_discovery, no_device_create };

const struct lowlevel_plugin* usb_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* usbft601_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* pcie_uram_register(void) { return &s_sim_plugin; }
