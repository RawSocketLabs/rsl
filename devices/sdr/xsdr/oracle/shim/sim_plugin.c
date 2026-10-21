// libusdr low-level transport that forwards every operation to xsdr-sim (Rust).
//
// libusdr's plugin list is fixed at build time (lowlevel/usdr_lowlevel.c), so this file
// supplies the three registration functions it calls: the USB entries never match a
// device, and the PCIe entry returns the simulated LMS7 family (m2_lm7_1) board.
//
// Pattern copied from the uSDR oracle's shim (devices/sdr/usdr/oracle/shim/sim_plugin.c),
// trimmed to the control plane and the RX and TX streams: no PCIe layout or ABI helpers.

#include <errno.h>
#include <setjmp.h>
#include <signal.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "usdr_lowlevel.h"
#include "device/device.h"
#include "device/device_ids.h"
#include "hw/lms7002m/lms7002m.h"
#include "ipblks/streams/streams_api.h"
#include "dm_dev.h"

// Implemented in src/lib.rs. Returns 0 or a negative errno.
int xsdr_oracle_ls_op(unsigned op, unsigned addr, size_t insz, void* pin, size_t outsz,
                          const void* pout);
void xsdr_oracle_sleep_us(unsigned long long us);
// Fills `out` (`len` bytes) with the board's next RX block and `oob` with its two words.
// Returns 0, or -ETIMEDOUT while the board's stream engine is stopped.
int xsdr_oracle_rx_next(void* out, size_t len, unsigned long long* oob);
// Sets the board's TX buffers up, `block` bytes each.
void xsdr_oracle_tx_open(size_t block);
// Releases the board's TX buffers.
void xsdr_oracle_tx_close(void);
// Takes the board's next free TX buffer: 0 with `*has_record` set and `words` filled when
// a played packet freed it, or -ETIMEDOUT while every buffer waits to play.
int xsdr_oracle_tx_grant(unsigned* words, int* has_record);
// Hands the bytes libusdr wrote into the granted buffer to the board.
void xsdr_oracle_tx_load(const void* bytes, size_t len);

// Replaces libc's usleep so libusdr's delays advance the board's virtual clock instead.
int usleep(useconds_t us)
{
    xsdr_oracle_sleep_us(us);
    return 0;
}

struct sim_dev {
    lowlevel_dev_t ll;  // must be first: libusdr casts lldev_t to it
    device_id_t id;
    void* rx_block;     // the RX block lent to libusdr between wait and release
    size_t rx_block_size;
    void* tx_block;     // the TX buffer lent to libusdr between get and commit
    size_t tx_block_size;
    void* tx_param;     // the TX stream's commit callback and its argument
    soft_tx_commit_fn_t tx_commit;
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
    free(((struct sim_dev*)dev)->rx_block);
    free(((struct sim_dev*)dev)->tx_block);
    free(dev);
    return 0;
}

// The PCIe kernel driver's DMA limits, as the uSDR shim states them (dmacap 0x855, the
// XSDR's `/ll/srx/0/dmacap` too): exactly 32 buffers, each at most 1 MiB.
enum { SIM_DMA_BUFFERS = 32, SIM_DMA_MAX_BLOCK = 1 << 20 };

// Stream setup as the PCIe transport reports it: stream 0 is RX, 1 TX. The kernel's own
// register writes sit below the ls_op seam xsdr shares, so they are not traced.
static int sim_stream_initialize(lldev_t dev, subdev_t subdev, lowlevel_stream_params_t* params,
                                 stream_t* channel)
{
    (void)subdev;
    struct sim_dev* d = (struct sim_dev*)dev;
    if (params->streamno > 1 || params->buffer_count != SIM_DMA_BUFFERS ||
        params->block_size == 0 || params->block_size > SIM_DMA_MAX_BLOCK) {
        return -EINVAL;
    }
    void** slot = params->streamno == 0 ? &d->rx_block : &d->tx_block;
    void* block = realloc(*slot, params->block_size);
    if (block == NULL) {
        return -ENOMEM;
    }
    *slot = block;
    if (params->streamno == 0) {
        d->rx_block_size = params->block_size;
    } else {
        d->tx_block_size = params->block_size;
        d->tx_param = params->param;
        d->tx_commit = params->soft_tx_commit;
        xsdr_oracle_tx_open(params->block_size);
    }
    params->underlying_fd = -1;
    params->out_mtu_size = params->block_size;
    *channel = params->streamno;
    return 0;
}

static int sim_stream_deinitialize(lldev_t dev, subdev_t subdev, stream_t channel)
{
    (void)dev;
    (void)subdev;
    if (channel == 1) {
        xsdr_oracle_tx_close();
    }
    return 0;
}

// The board's next free TX buffer, or a timeout at once while every buffer waits to play.
// Like the PCIe transport's, the out-of-band record is the driver's completion record when
// there is one, and empty (size 0) for the buffers it starts with.
static int sim_send_dma_get(lldev_t dev, subdev_t subdev, stream_t channel, void** buffer,
                            void* oob_ptr, unsigned* oob_size, unsigned timeout)
{
    (void)subdev;
    (void)channel;
    (void)timeout;
    struct sim_dev* d = (struct sim_dev*)dev;
    unsigned words[4] = { 0, 0, 0, 0 };
    int has_record = 0;
    int res = d->tx_block == NULL ? -ETIMEDOUT : xsdr_oracle_tx_grant(words, &has_record);
    if (res) {
        return res;
    }
    *buffer = d->tx_block;
    if (oob_ptr != NULL && oob_size != NULL) {
        if (has_record && *oob_size >= sizeof(words)) {
            memcpy(oob_ptr, words, sizeof(words));
            *oob_size = sizeof(words);
        } else {
            *oob_size = 0;
        }
    }
    return 0;
}

// As the PCIe transport's commit: the size check, then the stream's own commit, which
// announces the buffer through the TX core's registers. The bytes reach the board first.
static int sim_send_dma_commit(lldev_t dev, subdev_t subdev, stream_t channel, void* buffer,
                               unsigned sz, const void* oob_ptr, unsigned oob_size)
{
    (void)subdev;
    (void)channel;
    struct sim_dev* d = (struct sim_dev*)dev;
    if (buffer != d->tx_block || sz > d->tx_block_size || d->tx_commit == NULL) {
        return -EINVAL;
    }
    xsdr_oracle_tx_load(buffer, sz);
    return d->tx_commit(d->tx_param, sz, oob_ptr, oob_size);
}

// The board's next block, or a timeout at once while its stream engine is stopped. The
// out-of-band record is always written, so libusdr's timeout log reads defined memory.
static int sim_recv_dma_wait(lldev_t dev, subdev_t subdev, stream_t channel, void** buffer,
                             void* oob_ptr, unsigned* oob_size, unsigned timeout)
{
    (void)subdev;
    (void)channel;
    (void)timeout;
    struct sim_dev* d = (struct sim_dev*)dev;
    unsigned long long oob[2] = { 0, 0 };
    int res = d->rx_block == NULL
        ? -ETIMEDOUT
        : xsdr_oracle_rx_next(d->rx_block, d->rx_block_size, oob);
    *buffer = res == 0 ? d->rx_block : NULL;
    if (oob_ptr != NULL && oob_size != NULL) {
        unsigned size = *oob_size < sizeof(oob) ? *oob_size : (unsigned)sizeof(oob);
        memcpy(oob_ptr, oob, size);
        *oob_size = size;
    }
    return res;
}

static int sim_recv_dma_release(lldev_t dev, subdev_t subdev, stream_t channel, void* buffer)
{
    (void)dev;
    (void)subdev;
    (void)channel;
    (void)buffer;
    return 0;
}

// A stream option libusdr's public API does not reach, such as the TX stream's "mute"
// (the uSDR shim's pattern).
int xsdr_oracle_stream_option_set(void* stream, const char* name, long long value)
{
    stream_handle_t* h = (stream_handle_t*)stream;
    return h->ops->option_set(h, name, value);
}

static lowlevel_ops_t s_sim_ops = {
    .generic_get = sim_generic_get,
    .ls_op = sim_ls_op,
    .stream_initialize = sim_stream_initialize,
    .stream_deinitialize = sim_stream_deinitialize,
    .recv_dma_wait = sim_recv_dma_wait,
    .recv_dma_release = sim_recv_dma_release,
    .send_dma_get = sim_send_dma_get,
    .send_dma_commit = sim_send_dma_commit,
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
        case 32: results[i] = lms7002m_dc_corr(&m, op->a, (int16_t)op->b); break;
        case 33: results[i] = lms7002m_dc_corr_en(&m, op->a, op->b, op->c, op->d); break;
        case 34: results[i] = lms7002m_xxtsp_iq_gcorr(&m, (lms7002m_xxtsp_t)op->a, op->b, op->c); break;
        case 35: results[i] = lms7002m_xxtsp_iq_phcorr(&m, (lms7002m_xxtsp_t)op->a, (int)op->b); break;
        case 36: results[i] = lms7002m_xxtsp_gen(&m, (lms7002m_xxtsp_t)op->a, (lms7002m_xxtsp_gen_t)op->b, (int16_t)op->c, (int16_t)op->d); break;
        case 37: results[i] = lms7002m_limelight_toggle_ntx(&m); break;
        case 38: results[i] = lms7002m_limelight_toggle_tsp_clk(&m, unpack_conf(op->a, op->b), op->c); break;
        case 39: results[i] = lms7002m_limelight_upd_delay(&m, unpack_conf(op->a, op->b)); break;
        default: return -EINVAL;
        }
    }
    return 0;
}

// Where a trapped libusdr call resumes: the thread that made the call is the one SIGFPE
// interrupts (the division is synchronous), and oracles are serialised.
static __thread sigjmp_buf s_trap;

static void on_division_trap(int sig)
{
    (void)sig;
    siglongjmp(s_trap, 1);
}

// usdr_dme_set_uint(dev, path, value), with an integer division by zero (SIGFPE) caught:
// then *trapped is 1, and libusdr's call is abandoned where it divided, its frames above
// unwound without running. libusdr takes no lock on this path, and the frames hold no
// allocation, so the device stays usable. No Rust frame is on the stack at the division:
// it is libusdr's own arithmetic, between its bus operations. The SIGFPE handler is
// process-wide while the call runs: that is safe only because oracle sessions are
// serialised by the oracle's lock, so no other thread runs libusdr (or expects SIGFPE's
// default action) meanwhile.
int xsdr_oracle_set_uint_trapping(pdm_dev_t dev, const char* path, uint64_t value, int* trapped)
{
    struct sigaction trap, previous;
    memset(&trap, 0, sizeof(trap));
    trap.sa_handler = on_division_trap;
    sigemptyset(&trap.sa_mask);
    sigaction(SIGFPE, &trap, &previous);
    int res = 0;
    if (sigsetjmp(s_trap, 1) == 0) {
        res = usdr_dme_set_uint(dev, path, value);
        *trapped = 0;
    } else {
        *trapped = 1;
    }
    sigaction(SIGFPE, &previous, NULL);
    return res;
}

static const struct lowlevel_plugin s_sim_plugin = { sim_info_str, sim_discovery, sim_create };
static const struct lowlevel_plugin s_no_device = { sim_info_str, sim_discovery, no_device_create };

const struct lowlevel_plugin* usb_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* usbft601_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* pcie_uram_register(void) { return &s_sim_plugin; }
