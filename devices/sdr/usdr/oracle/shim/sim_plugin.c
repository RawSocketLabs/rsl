// libusdr low-level transport that forwards every operation to rsl-usdr-sim (Rust).
//
// libusdr's plugin list is fixed at build time (lowlevel/usdr_lowlevel.c), so this file
// supplies the three registration functions it calls: the USB entries never match a
// device, and the PCIe entry returns a simulated uSDR (m2_lm6_1) board.
//
// Several boards can be open at once, as libusdr's multi-device aggregate (`device/mdev.c`)
// opens them: its connection string `bus=sim0:sim1:...` makes `mdev_create` call this
// plugin once per bus name, and `simN` selects the oracle's board N. Without a `bus`
// parameter the plugin opens board 0. Every callback names its board.

#include <ctype.h>
#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "usdr_lowlevel.h"
#include "device/device.h"
#include "device/device_bus.h"
#include "device/device_ids.h"
#include "device/device_names.h"
#include "device/device_vfs.h"
#include "lowlevel/pcie_uram/pcie_uram_driver_if.h"
#include "ipblks/si2c.h"
#include "ipblks/espi_flash.h"
#include "ipblks/xlnx_bitstream.h"
#include "ipblks/streams/streams_api.h"

// Stateless oracle for the USB transport's simple-I2C command/cache sequence.
// Each request is two words: addr|bus<<8|outlen<<16|inlen<<24, then LE outgoing bytes.
// Output is two words per request: the hardware LUT and the control word.
int rsl_oracle_si2c_sequence(const unsigned* requests, size_t count, unsigned* out)
{
    struct i2c_cache cache[4] = {0};
    for (size_t i = 0; i < count; i++) {
        unsigned shape = requests[2 * i];
        unsigned data = requests[2 * i + 1];
        unsigned char bytes[3] = {data & 255, (data >> 8) & 255, (data >> 16) & 255};
        unsigned slot = si2c_update_lut_idx(cache, shape & 127, (shape >> 8) & 1);
        out[2 * i] = si2c_get_lut(cache);
        int res = si2c_make_ctrl_reg(slot, bytes, (shape >> 16) & 255,
                                    (shape >> 24) & 255, &out[2 * i + 1]);
        if (res)
            return res;
    }
    return 0;
}

// Implemented in src/lib.rs, each on the oracle's board `board`. Returns 0 or a negative
// errno.
int rsl_oracle_ls_op(unsigned board, unsigned op, unsigned addr, size_t insz, void* pin,
                     size_t outsz, const void* pout);
// Advances the clock of the board libusdr addressed last: `usleep` names no device.
void rsl_oracle_sleep_us(unsigned long long us);
// Fills `out` (`len` bytes) with the board's next RX block and `oob` with its two words.
// Returns 0, or -ETIMEDOUT while the board's stream engine is stopped.
int rsl_oracle_rx_next(unsigned board, void* out, size_t len, unsigned long long* oob);
// Sets the board's TX buffers up, `block` bytes each.
void rsl_oracle_tx_open(unsigned board, size_t block);
// Releases the board's TX buffers.
void rsl_oracle_tx_close(unsigned board);
// Takes the board's next free TX buffer: 0 with `*has_record` set and `words` filled when
// a played packet freed it, or -ETIMEDOUT while every buffer waits to play.
int rsl_oracle_tx_grant(unsigned board, unsigned* words, int* has_record);
// Hands the bytes libusdr wrote into the granted buffer to the board.
void rsl_oracle_tx_load(unsigned board, const void* bytes, size_t len);

// Replaces libc's usleep so libusdr's delays advance the board's virtual clock instead.
int usleep(useconds_t us)
{
    rsl_oracle_sleep_us(us);
    return 0;
}

struct sim_dev {
    lowlevel_dev_t ll;  // must be first: libusdr casts lldev_t to it
    device_id_t id;
    unsigned board;     // the oracle's board this device forwards to
    void* rx_block;    // the RX block lent to libusdr between wait and release
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
    case LLGO_DEVICE_SDR_TYPE: *pout = (const char*)SDR_USDR; return 0;
    }
    return -EINVAL;
}

static int sim_ls_op(lldev_t dev, subdev_t subdev, unsigned op, lsopaddr_t addr,
                     size_t meminsz, void* pin, size_t memoutsz, const void* pout)
{
    (void)subdev;
    return rsl_oracle_ls_op(((struct sim_dev*)dev)->board, op, addr, meminsz, pin, memoutsz,
                            pout);
}

static int sim_destroy(lldev_t dev)
{
    free(((struct sim_dev*)dev)->rx_block);
    free(((struct sim_dev*)dev)->tx_block);
    if (dev->pdev) {
        dev->pdev->destroy(dev->pdev);
    }
    free(dev);
    return 0;
}

// The PCIe kernel driver's DMA limits (usdr_pcie_uram.c, dmacap 0x855): exactly 32 buffers,
// each at most 1 MiB.
enum { SIM_DMA_BUFFERS = 32, SIM_DMA_MAX_BLOCK = 1 << 20 };

// Stream setup as the PCIe transport reports it (pcie_uram_main.c stream_initialize): stream
// 0 is RX, 1 TX. The kernel's own register writes (buffer addresses, block size) sit below
// the ls_op seam that rsl-usdr shares, so they are not traced.
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
        rsl_oracle_tx_open(d->board, params->block_size);
    }
    // The PCIe transport reports its device's descriptor (`d->fd`), and libusdr's
    // multi-device streams refuse a stream without one (`_mdev_create_stream`: -ENOTSUP).
    // Nothing at the pinned commit polls it (`mdev.c` only stores it), so the sim reports a
    // placeholder, descriptor 0, which nothing reads.
    params->underlying_fd = 0;
    params->out_mtu_size = params->block_size;
    *channel = params->streamno;
    return 0;
}

static int sim_stream_deinitialize(lldev_t dev, subdev_t subdev, stream_t channel)
{
    (void)subdev;
    if (channel == 1) {
        rsl_oracle_tx_close(((struct sim_dev*)dev)->board);
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
    int res = d->tx_block == NULL ? -ETIMEDOUT : rsl_oracle_tx_grant(d->board, words, &has_record);
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
    rsl_oracle_tx_load(d->board, buffer, sz);
    return d->tx_commit(d->tx_param, sz, oob_ptr, oob_size);
}

// A stream option libusdr's public API does not reach, such as the TX stream's "mute".
int rsl_oracle_stream_option_set(void* stream, const char* name, long long value)
{
    stream_handle_t* h = (stream_handle_t*)stream;
    return h->ops->option_set(h, name, value);
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
        : rsl_oracle_rx_next(d->board, d->rx_block, d->rx_block_size, oob);
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

// The device the sim plugin created last, for the PCIe layout check below.
static struct sim_dev* s_last_dev;

// Set while libusdr's flash tool runs: see rsl_oracle_usdr_flash below.
static int s_tool_mode;

// A tool-mode device's teardown: its description only, no power-down.
static int sim_tool_destroy(lldev_t dev)
{
    if (dev->pdev) {
        usdr_device_base_destroy(dev->pdev);
        free(dev->pdev);
    }
    free(dev);
    return 0;
}

static lowlevel_ops_t s_sim_tool_ops = {
    .generic_get = sim_generic_get,
    .ls_op = sim_ls_op,
    .destroy = sim_tool_destroy,
};

// The board a `bus` parameter names: `simN` is board N (at most 31, libusdr's aggregate
// holds 32), and no `bus` parameter board 0. Returns -ENODEV for any other bus name.
static int sim_board_of(unsigned pcount, const char** devparam, const char** devval)
{
    for (unsigned i = 0; i < pcount; i++) {
        if (strcmp(devparam[i], "bus") != 0) {
            continue;
        }
        const char* name = devval[i];
        if (name == NULL || strncmp(name, "sim", 3) != 0 || !isdigit((unsigned char)name[3])) {
            return -ENODEV;
        }
        char* end = NULL;
        unsigned long board = strtoul(name + 3, &end, 10);
        if (*end != 0 || board > 31) {
            return -ENODEV;
        }
        return (int)board;
    }
    return 0;
}

static int sim_create(unsigned pcount, const char** devparam, const char** devval,
                      lldev_t* odev, unsigned vidpid, void* webops, uintptr_t param)
{
    (void)vidpid; (void)webops; (void)param;
    int board = sim_board_of(pcount, devparam, devval);
    if (board < 0) {
        return board;
    }
    struct sim_dev* d = calloc(1, sizeof(*d));
    if (d == NULL) {
        return -ENOMEM;
    }
    d->ll.ops = s_tool_mode ? &s_sim_tool_ops : &s_sim_ops;
    d->id = M2_LM6_1_DEVICE_ID_C;
    d->board = (unsigned)board;

    // Same order as pcie_uram_plugin_create; like it, a failed init frees only the handle.
    // The flash tool's device gets its description but no bring-up (see below).
    int err = usdr_device_create(&d->ll, d->id);
    if (!s_tool_mode) {
        err = err ? err : d->ll.pdev->initialize(d->ll.pdev, pcount, devparam, devval);
    }
    if (err) {
        free(d);
        return err;
    }
    *odev = &d->ll;
    if (!s_tool_mode) {
        s_last_dev = d;
    }
    return 0;
}

// The `pcie_driver_devlayout` libusdr's PCIe transport would send for the open board, built
// as `pcie_uram_plugin_create` does (pcie_uram_main.c:970-1048, MIT) from the board's own
// description. One difference: libusdr copies the indexed-window arrays with the size of
// the four-entry destination, reading past the two-entry source; here only the populated
// entries are copied, the ones the driver reads (it uses `idx_regsp_cnt` of them).
int rsl_oracle_pcie_devlayout(unsigned char* out, size_t len)
{
    if (s_last_dev == NULL || len != sizeof(struct pcie_driver_devlayout)) {
        return -EINVAL;
    }
    pdevice_t pdev = s_last_dev->ll.pdev;
    // Zeroed as libusdr's is (it lives in the calloc'd transport state): the unused slots
    // are copied too.
    device_bus_t db;
    memset(&db, 0, sizeof(db));
    int err = device_bus_init(pdev, &db);
    if (err) {
        return err;
    }
    uint64_t tmp;
    struct pcie_driver_devlayout dl;
    memset(&dl, 0, sizeof(dl));

    dl.spi_cnt = db.spi_count;
    dl.i2c_cnt = db.i2c_count;
    dl.idx_regsp_cnt = db.idx_regsps;
    dl.streams_count = db.srx_count + db.stx_count;
    if (db.bucket_count != 1 || dl.idx_regsp_cnt > DBMAX_IDXREG_MAPS) {
        return -ENOSPC;
    }
    memcpy(dl.idx_regsp_base, db.idxreg_base, dl.idx_regsp_cnt * sizeof(dl.idx_regsp_base[0]));
    memcpy(dl.idx_regsp_vbase, db.idxreg_virt_base, dl.idx_regsp_cnt * sizeof(dl.idx_regsp_vbase[0]));
    memcpy(dl.spi_base, db.spi_base, sizeof(dl.spi_base));
    memcpy(dl.i2c_base, db.i2c_base, sizeof(dl.i2c_base));
    memcpy(dl.spi_core, db.spi_core, sizeof(dl.spi_core));
    memcpy(dl.i2c_core, db.i2c_core, sizeof(dl.i2c_core));

    memcpy(dl.stream_cnf_base, db.srx_base, db.srx_count * sizeof(dl.stream_cnf_base[0]));
    memcpy(dl.stream_cnf_base + db.srx_count, db.stx_base, db.stx_count * sizeof(dl.stream_cnf_base[0]));
    memcpy(dl.stream_cfg_base, db.srx_cfg_base, db.srx_count * sizeof(dl.stream_cfg_base[0]));
    memcpy(dl.stream_cfg_base + db.srx_count, db.stx_cfg_base, db.stx_count * sizeof(dl.stream_cfg_base[0]));
    memcpy(dl.stream_core, db.srx_core, db.srx_count * sizeof(dl.stream_core[0]));
    memcpy(dl.stream_core + db.srx_count, db.stx_core, db.stx_count * sizeof(dl.stream_core[0]));

    err = usdr_device_vfs_obj_val_get_u64(pdev, DNLL_IRQ_COUNT, &tmp);
    if (err)
        return err;
    dl.interrupt_count = tmp;

    err = usdr_device_vfs_obj_val_get_u64(pdev, DNLLFP_BASE(DNP_IRQ, "0"), &tmp);
    if (err)
        return err;
    dl.interrupt_base = tmp;

    dl.poll_event_rd = db.poll_event_rd;
    dl.poll_event_wr = db.poll_event_wr;

    struct device_params {
        const char* path;
        unsigned* store;
        unsigned count;
    } bii[] = {
        { DNLLFP_IRQ(DN_BUS_SPI, "%d"), dl.spi_int_number, dl.spi_cnt },
        { DNLLFP_IRQ(DN_BUS_I2C, "%d"), dl.i2c_int_number, dl.i2c_cnt },
        { DNLLFP_IRQ(DN_SRX, "%d"), dl.stream_int_number, db.srx_count },
        { DNLLFP_IRQ(DN_STX, "%d"), dl.stream_int_number + db.srx_count, db.stx_count },
        { DNLLFP_NAME(DN_SRX, "%d", DNP_DMACAP), dl.stream_cap, db.srx_count },
        { DNLLFP_NAME(DN_STX, "%d", DNP_DMACAP), dl.stream_cap + db.srx_count, db.stx_count },
    };
    char buffer[32];
    for (unsigned i = 0; i < sizeof(bii) / sizeof(bii[0]); i++) {
        for (unsigned j = 0; j < bii[i].count; j++) {
            snprintf(buffer, sizeof(buffer), bii[i].path, j);
            err = usdr_device_vfs_obj_val_get_u64(pdev, buffer, &tmp);
            if (err)
                return err;
            bii[i].store[j] = (unsigned)tmp;
        }
    }

    dl.bucket_base = db.bucket_base[0];
    dl.bucket_core = db.bucket_core[0];
    dl.bucket_count = db.bucket_count;

    memcpy(out, &dl, sizeof(dl));
    return 0;
}

// The driver interface's sizes, offsets and request codes, from the header itself, each
// named by the C expression that produced it, so rsl-usdr-pcie's facts are matched by name.
struct rsl_abi_fact {
    const char* name;
    unsigned long long value;
};

#define ABI_SIZE(t) { "sizeof(" #t ")", sizeof(struct t) }
#define ABI_OFFSET(t, f) { "offsetof(" #t ", " #f ")", offsetof(struct t, f) }
#define ABI_CODE(c) { #c, c }

static const struct rsl_abi_fact s_pcie_abi[] = {
    ABI_SIZE(pcie_driver_uuid),
    ABI_SIZE(pcie_driver_devlayout),
    ABI_SIZE(pcie_driver_spi32),
    ABI_SIZE(pcie_driver_si2c),
    ABI_OFFSET(pcie_driver_si2c, rdb),
    ABI_OFFSET(pcie_driver_si2c, wrb_p),
    ABI_SIZE(pcie_driver_sdma_conf),
    ABI_OFFSET(pcie_driver_sdma_conf, out_vma_off),
    ABI_OFFSET(pcie_driver_sdma_conf, out_vma_length),
    ABI_SIZE(pcie_driver_woa_oob),
    ABI_OFFSET(pcie_driver_woa_oob, oobdata),
    ABI_OFFSET(pcie_driver_devlayout, idx_regsp_vbase),
    ABI_OFFSET(pcie_driver_devlayout, stream_cap),
    ABI_OFFSET(pcie_driver_devlayout, bucket_base),
    ABI_CODE(PCIE_DRIVER_GET_UUID),
    ABI_CODE(PCIE_DRIVER_CLAIM),
    ABI_CODE(PCIE_DRIVER_SET_DEVLAYOUT),
    ABI_CODE(PCIE_DRIVER_SPI32_TRANSACT),
    ABI_CODE(PCIE_DRIVER_SI2C_TRANSACT),
    ABI_CODE(PCIE_DRIVER_DMA_CONF),
    ABI_CODE(PCIE_DRIVER_DMA_UNCONF),
    ABI_CODE(PCIE_DRIVER_DMA_WAIT_OOB),
    ABI_CODE(PCIE_DRIVER_DMA_ALLOC_OOB),
    ABI_CODE(PCIE_DRIVER_CLAIM_VERSION),
    ABI_CODE(PCIE_DRIVER_DMA_RELEASE),
    ABI_CODE(PCIE_FLASH_READ),
    ABI_CODE(PCIE_FLASH_ERASE),
    ABI_CODE(PCIE_FLASH_WRITE),
    ABI_CODE(PCIE_GETIDS),
};

const struct rsl_abi_fact* rsl_oracle_pcie_abi(size_t* count)
{
    *count = sizeof(s_pcie_abi) / sizeof(s_pcie_abi[0]);
    return s_pcie_abi;
}

static int no_device_create(unsigned pcount, const char** devparam, const char** devval,
                            lldev_t* odev, unsigned vidpid, void* webops, uintptr_t param)
{
    (void)pcount; (void)devparam; (void)devval; (void)odev; (void)vidpid; (void)webops; (void)param;
    return -ENODEV;
}

// libusdr's flash driver against board 0 directly. It needs only `ls_op`, so the device
// is a bare transport handle with no board description and no bring-up: the parity tests
// compare the flash operations alone. The register bases are the caller's.
static struct sim_dev s_flash_sim = { .ll = { .ops = &s_sim_ops, .pdev = NULL }, .board = 0 };
static lowlevel_dev_t* const s_flash_dev = &s_flash_sim.ll;

int rsl_oracle_flash_get_id(unsigned base, uint32_t* id, char* out, size_t maxstr)
{
    return espi_flash_get_id(s_flash_dev, 0, base, id, out, maxstr);
}

int rsl_oracle_flash_read(unsigned base, unsigned mmap_base, uint32_t offset, uint32_t size,
                          uint8_t* out)
{
    return espi_flash_read(s_flash_dev, 0, base, mmap_base, offset, size, out);
}

int rsl_oracle_flash_erase(unsigned base, uint32_t size, uint32_t offset)
{
    return espi_flash_erase(s_flash_dev, 0, base, size, offset);
}

int rsl_oracle_flash_write(unsigned base, unsigned mmap_base, const uint8_t* in, uint32_t size,
                           uint32_t offset, unsigned flags)
{
    return espi_flash_write(s_flash_dev, 0, base, mmap_base, in, size, offset, flags);
}

// libusdr's flash tool (tools/usdr_flash.c, its `main` renamed by build.rs) on the board,
// with `argv` as its command line and `reply` as what is typed at its prompt; its log goes
// to a buffer the caller frees. Returns the tool's exit status. On hardware the tool's
// lowlevel_create also brings the board up (and lowlevel_destroy powers it down); here its
// device gets the board description (the flash base and master offset the tool reads) and
// nothing else, so the board's trace holds the tool's own operations. rsl-usdr's flash
// maintenance powers nothing either. Returns RSL_ORACLE_TOOL_SETUP_FAILED, which the tool
// never does (its statuses are 0 to 9), with no log if its streams cannot be set up.
int rsl_oracle_usdr_flash_main(int argc, char** argv);

enum { RSL_ORACLE_TOOL_SETUP_FAILED = -1 };

int rsl_oracle_usdr_flash(int argc, char** argv, const char* reply, char** log, size_t* log_len)
{
    FILE* in = fmemopen((void*)reply, strlen(reply) + 1, "r");
    FILE* err = open_memstream(log, log_len);
    if (in == NULL || err == NULL) {
        if (in) {
            fclose(in);
        }
        if (err) {
            fclose(err);
            free(*log);
        }
        *log = NULL;
        return RSL_ORACLE_TOOL_SETUP_FAILED;
    }
    FILE* saved_in = stdin;
    FILE* saved_err = stderr;
    stdin = in;
    stderr = err;
    optind = 0; // glibc: a full rescan, as on a fresh process
    s_tool_mode = 1;
    int status = rsl_oracle_usdr_flash_main(argc, argv);
    s_tool_mode = 0;
    stdin = saved_in;
    stderr = saved_err;
    fclose(in);
    fclose(err);
    return status;
}

// Sets `res` to `call`'s result with libusdr's log lines discarded: the parity tests feed the image header
// parser and update check thousands of images they refuse.
#define QUIETLY(call)                                \
    do {                                             \
        FILE* saved = stderr;                        \
        FILE* sink = fopen("/dev/null", "w");        \
        if (sink) {                                  \
            stderr = sink;                           \
        }                                            \
        res = (call);                                \
        stderr = saved;                              \
        if (sink) {                                  \
            fclose(sink);                            \
        }                                            \
    } while (0)

int rsl_oracle_btstrm_parse(const uint32_t* mem, unsigned len, xlnx_image_params_t* stat,
                            unsigned flags)
{
    int res;
    QUIETLY(xlnx_btstrm_parse_header_ex(mem, len, stat, flags));
    return res;
}

int rsl_oracle_btstrm_iprgcheck(const xlnx_image_params_t* golden,
                                const xlnx_image_params_t* image, unsigned wbstar,
                                bool golden_image)
{
    int res;
    QUIETLY(xlnx_btstrm_iprgcheck(golden, image, wbstar, golden_image));
    return res;
}

static const struct lowlevel_plugin s_sim_plugin = { sim_info_str, sim_discovery, sim_create };
static const struct lowlevel_plugin s_no_device = { sim_info_str, sim_discovery, no_device_create };

const struct lowlevel_plugin* usb_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* usbft601_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* pcie_uram_register(void) { return &s_sim_plugin; }
