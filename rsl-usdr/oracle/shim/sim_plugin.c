// libusdr low-level transport that forwards every operation to rsl-usdr-sim (Rust).
//
// libusdr's plugin list is fixed at build time (lowlevel/usdr_lowlevel.c), so this file
// supplies the three registration functions it calls: the USB entries never match a
// device, and the PCIe entry returns the simulated uSDR (m2_lm6_1) board.

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

// Implemented in src/lib.rs. Returns 0 or a negative errno.
int rsl_oracle_ls_op(unsigned op, unsigned addr, size_t insz, void* pin, size_t outsz, const void* pout);
void rsl_oracle_sleep_us(unsigned long long us);
// Fills `out` (`len` bytes) with the board's next RX block and `oob` with its two words.
// Returns 0, or -ETIMEDOUT while the board's stream engine is stopped.
int rsl_oracle_rx_next(void* out, size_t len, unsigned long long* oob);

// Replaces libc's usleep so libusdr's delays advance the board's virtual clock instead.
int usleep(useconds_t us)
{
    rsl_oracle_sleep_us(us);
    return 0;
}

struct sim_dev {
    lowlevel_dev_t ll;  // must be first: libusdr casts lldev_t to it
    device_id_t id;
    void* rx_block;     // the RX block lent to libusdr between wait and release
    size_t rx_block_size;
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
    (void)dev;
    (void)subdev;
    return rsl_oracle_ls_op(op, addr, meminsz, pin, memoutsz, pout);
}

static int sim_destroy(lldev_t dev)
{
    free(((struct sim_dev*)dev)->rx_block);
    if (dev->pdev) {
        dev->pdev->destroy(dev->pdev);
    }
    free(dev);
    return 0;
}

// The PCIe kernel driver's DMA limits (usdr_pcie_uram.c, dmacap 0x855): exactly 32 buffers,
// each at most 1 MiB.
enum { SIM_DMA_BUFFERS = 32, SIM_DMA_MAX_BLOCK = 1 << 20 };

// Stream setup as the PCIe transport reports it (pcie_uram_main.c stream_initialize). The
// kernel's own register writes (buffer addresses, block size) sit below the ls_op seam that
// rsl-usdr shares, so they are not traced.
static int sim_stream_initialize(lldev_t dev, subdev_t subdev, lowlevel_stream_params_t* params,
                                 stream_t* channel)
{
    (void)subdev;
    struct sim_dev* d = (struct sim_dev*)dev;
    if (params->buffer_count != SIM_DMA_BUFFERS || params->block_size == 0 ||
        params->block_size > SIM_DMA_MAX_BLOCK) {
        return -EINVAL;
    }
    void* block = realloc(d->rx_block, params->block_size);
    if (block == NULL) {
        return -ENOMEM;
    }
    d->rx_block = block;
    d->rx_block_size = params->block_size;
    params->underlying_fd = -1;
    params->out_mtu_size = params->block_size;
    *channel = params->streamno;
    return 0;
}

static int sim_stream_deinitialize(lldev_t dev, subdev_t subdev, stream_t channel)
{
    (void)dev;
    (void)subdev;
    (void)channel;
    return 0;
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
        : rsl_oracle_rx_next(d->rx_block, d->rx_block_size, oob);
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

static int sim_create(unsigned pcount, const char** devparam, const char** devval,
                      lldev_t* odev, unsigned vidpid, void* webops, uintptr_t param)
{
    (void)vidpid; (void)webops; (void)param;
    struct sim_dev* d = calloc(1, sizeof(*d));
    if (d == NULL) {
        return -ENOMEM;
    }
    d->ll.ops = &s_sim_ops;
    d->id = M2_LM6_1_DEVICE_ID_C;

    // Same order as pcie_uram_plugin_create; like it, a failed init frees only the handle.
    int err = usdr_device_create(&d->ll, d->id);
    err = err ? err : d->ll.pdev->initialize(d->ll.pdev, pcount, devparam, devval);
    if (err) {
        free(d);
        return err;
    }
    *odev = &d->ll;
    s_last_dev = d;
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

// The driver interface's sizes, offsets and request codes, from the header itself, in the
// order rsl-usdr-pcie's `abi()` lists them.
size_t rsl_oracle_pcie_abi(unsigned long long* out, size_t len)
{
    const unsigned long long facts[] = {
        sizeof(struct pcie_driver_uuid),
        sizeof(struct pcie_driver_devlayout),
        sizeof(struct pcie_driver_spi32),
        sizeof(struct pcie_driver_si2c),
        offsetof(struct pcie_driver_si2c, rdb),
        offsetof(struct pcie_driver_si2c, wrb_p),
        sizeof(struct pcie_driver_sdma_conf),
        offsetof(struct pcie_driver_sdma_conf, out_vma_off),
        offsetof(struct pcie_driver_sdma_conf, out_vma_length),
        sizeof(struct pcie_driver_woa_oob),
        offsetof(struct pcie_driver_woa_oob, oobdata),
        offsetof(struct pcie_driver_devlayout, idx_regsp_vbase),
        offsetof(struct pcie_driver_devlayout, stream_cap),
        offsetof(struct pcie_driver_devlayout, bucket_base),
        PCIE_DRIVER_GET_UUID,
        PCIE_DRIVER_CLAIM,
        PCIE_DRIVER_SET_DEVLAYOUT,
        PCIE_DRIVER_SPI32_TRANSACT,
        PCIE_DRIVER_SI2C_TRANSACT,
        PCIE_DRIVER_DMA_CONF,
        PCIE_DRIVER_DMA_UNCONF,
        PCIE_DRIVER_DMA_WAIT_OOB,
        PCIE_DRIVER_CLAIM_VERSION,
        PCIE_DRIVER_DMA_RELEASE,
    };
    size_t count = sizeof(facts) / sizeof(facts[0]);
    for (size_t i = 0; i < count && i < len; i++) {
        out[i] = facts[i];
    }
    return count;
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
const struct lowlevel_plugin* usbft601_uram_register(void) { return &s_no_device; }
const struct lowlevel_plugin* pcie_uram_register(void) { return &s_sim_plugin; }
