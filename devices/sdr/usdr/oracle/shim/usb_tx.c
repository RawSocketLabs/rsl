// Hardware-free buffer boundary for the original pinned USB TX functions.
// The reference includes are generated verbatim; header framing is not reproduced here.
#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "usb_uram/usb_uram_generic.h"

// Standard endpoint-direction constants are the only libusb declarations used here.
#define LIBUSB_ENDPOINT_OUT 0x00
#define LIBUSB_ENDPOINT_IN 0x80

#undef USDR_LOG
#define USDR_LOG(...) ((void)0)

#include "../../../../vendor/libusdr/reference/usb_tx_params.inc"

// The fake owns plain memory instead of USB transfers. `ready` is the free-buffer count
// libusdr's OUT semaphore holds: every buffer at first, one more per finished transfer.
// Waiting on it fails as `usdr_sem_wait_ex` does: `sem_trywait`'s EAGAIN for a zero
// timeout, ETIMEDOUT for any other.
struct buffers {
    unsigned buf_max;
    unsigned allocsz;
    unsigned allocsz_rounded;
    int fd_event;
    uint32_t bufno_prod;
    uint32_t bufno_cons;
    unsigned ready;
    unsigned char* memory;
    unsigned requests;
    unsigned endpoint;
    unsigned posts;
    unsigned posted_index;
    unsigned posted_length;
    unsigned closes;
};

// The device fields the TX functions use; the fake register file answers status reads.
typedef struct {
    struct lowlevel_dev lld;
    unsigned gdev;
    struct buffers rx_strms[1];
    struct buffers tx_strms[1];
    struct stream_params tx_strms_params[1];
    uint32_t tx_stat_prev[4];
    uint32_t tx_stat_cnt;
    uint32_t tx_stat_rate;
    uint32_t status[4];
    unsigned reads;
    unsigned read_addr;
    unsigned read_words;
} usb_dev_t;

static int fake_ls_op(lldev_t dev, subdev_t subdev, unsigned ls_op, lsopaddr_t addr,
                      size_t meminsz, void* pin, size_t memoutsz, const void* pout)
{
    (void)subdev;
    (void)memoutsz;
    (void)pout;
    usb_dev_t* d = (usb_dev_t*)dev;
    if (ls_op != USDR_LSOP_HWREG || meminsz > sizeof(d->status))
        return -EINVAL;
    d->reads++;
    d->read_addr = addr;
    d->read_words = meminsz / 4;
    memcpy(pin, d->status, meminsz);
    return 0;
}

static lowlevel_ops_t s_fake_ops = {.ls_op = fake_ls_op};

static void* buffers_get_ptr(struct buffers* buffers, unsigned index)
{
    return buffers->memory + index * buffers->allocsz_rounded;
}

static int buffers_usb_init(unsigned* gdev, struct buffers* buffers, unsigned requests,
                            unsigned count, unsigned block, unsigned endpoint, bool eventtype)
{
    (void)gdev;
    buffers->buf_max = count;
    buffers->allocsz = block;
    buffers->allocsz_rounded = (block + 4095) & ~4095u;
    buffers->fd_event = eventtype ? 1 : -1;
    buffers->bufno_prod = 0;
    buffers->bufno_cons = 0;
    buffers->ready = count;
    buffers->requests = requests;
    buffers->endpoint = endpoint;
    buffers->posts = 0;
    buffers->closes = 0;
    buffers->memory = calloc(count + 1, buffers->allocsz_rounded);
    return buffers->memory == NULL ? -ENOMEM : 0;
}

static int buffers_ready_wait(struct buffers* buffers, int64_t timeout_us)
{
    if (buffers->ready == 0)
        return timeout_us == 0 ? -EAGAIN : -ETIMEDOUT;
    buffers->ready--;
    return 0;
}

static unsigned buffers_produce(struct buffers* buffers)
{
    return (buffers->bufno_prod++) & (buffers->buf_max - 1);
}

static unsigned buffers_consume(struct buffers* buffers)
{
    return (buffers->bufno_cons++) & (buffers->buf_max - 1);
}

static int buffers_usb_transfer_post(struct buffers* buffers, unsigned index,
                                    unsigned length, unsigned transfer)
{
    (void)transfer;
    buffers->posts++;
    buffers->posted_index = index;
    buffers->posted_length = length;
    return 0;
}

static int buffers_usb_free(struct buffers* buffers)
{
    buffers->closes++;
    free(buffers->memory);
    buffers->memory = NULL;
    return 0;
}

#include "../../../../vendor/libusdr/reference/usb_tx.inc"

// A device as `usb_uram_plugin_create` leaves it for streams: zeroed, statistics started.
void* rsl_oracle_usb_tx_new(void)
{
    usb_dev_t* dev = calloc(1, sizeof(usb_dev_t));
    if (dev == NULL)
        return NULL;
    dev->lld.ops = &s_fake_ops;
    usb_async_start_tx_stat(dev);
    return dev;
}

void rsl_oracle_usb_tx_free(void* handle)
{
    usb_dev_t* dev = handle;
    free(dev->tx_strms[0].memory);
    free(dev);
}

// Opens TX as the stream engine does (32 buffers, no poll fd). `events`: transfers,
// buffers, buffer bytes, endpoint, MTU, channel.
int rsl_oracle_usb_tx_open(void* handle, unsigned block, unsigned bits_per_sym,
                           unsigned* events)
{
    usb_dev_t* dev = handle;
    lowlevel_stream_params_t params = {0};
    params.streamno = DEV_TX_STREAM_NO;
    params.block_size = block;
    params.buffer_count = 32;
    params.channels = 1;
    params.bits_per_sym = bits_per_sym;
    stream_t channel = 99;
    int res = _usb_uram_init_txstream(dev, &params, &channel);
    if (res)
        return res;
    struct buffers* buffers = &dev->tx_strms[0];
    events[0] = buffers->requests;
    events[1] = buffers->buf_max;
    events[2] = buffers->allocsz;
    events[3] = buffers->endpoint;
    events[4] = params.out_mtu_size;
    events[5] = channel;
    return 0;
}

// What the TX core's status registers hold for the next read.
void rsl_oracle_usb_tx_set_status(void* handle, const uint32_t* words)
{
    usb_dev_t* dev = handle;
    memcpy(dev->status, words, sizeof(dev->status));
}

// One send as `_sfetrx4_stream_send` makes it: get (16 status bytes asked, waiting up to
// `timeout_ms`), the payload written where the buffer was lent, commit with its time.
// Copies the posted frame out.
int rsl_oracle_usb_tx_send(void* handle, const unsigned char* payload, unsigned size,
                           uint64_t timestamp, unsigned timeout_ms, uint32_t* status,
                           unsigned* status_size, unsigned char* frame, unsigned* frame_size)
{
    usb_dev_t* dev = handle;
    void* buffer = NULL;
    unsigned oob_size = 16;
    int res = usb_uram_send_dma_get((lldev_t)dev, 0, DEV_TX_STREAM_NO, &buffer, status,
                                    &oob_size, timeout_ms);
    if (res)
        return res;
    *status_size = oob_size;
    memcpy(buffer, payload, size);
    uint64_t oob[3] = {timestamp, 0, size};
    res = usb_uram_send_dma_commit((lldev_t)dev, 0, DEV_TX_STREAM_NO, buffer, size, oob,
                                   sizeof(oob));
    if (res)
        return res;
    struct buffers* buffers = &dev->tx_strms[0];
    memcpy(frame, buffers_get_ptr(buffers, buffers->posted_index), buffers->posted_length);
    *frame_size = buffers->posted_length;
    return 0;
}

// The device finished the oldest transfer: libusb's callback posts its buffer back.
void rsl_oracle_usb_tx_finish(void* handle)
{
    usb_dev_t* dev = handle;
    dev->tx_strms[0].ready++;
}

// Status reads so far, with the last one's register and word count.
unsigned rsl_oracle_usb_tx_reads(void* handle, unsigned* addr, unsigned* words)
{
    usb_dev_t* dev = handle;
    *addr = dev->read_addr;
    *words = dev->read_words;
    return dev->reads;
}

// Closes TX; returns the stream's close count, or the error.
int rsl_oracle_usb_tx_close(void* handle)
{
    usb_dev_t* dev = handle;
    int res = usb_uram_stream_deinitialize((lldev_t)dev, 0, DEV_TX_STREAM_NO);
    return res ? res : (int)dev->tx_strms[0].closes;
}
