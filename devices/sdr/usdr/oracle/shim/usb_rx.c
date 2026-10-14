// Hardware-free buffer boundary for the original pinned USB RX functions.
// The reference include is generated verbatim; trailer parsing is not reproduced here.
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

struct buffers;
struct buffer_discriptor {
    struct buffers* b;
    unsigned bno;
    unsigned buffer_sz;
};

// The fake owns no USB resources: it lends the supplied frame and counts lifecycle actions.
struct buffers {
    unsigned buf_max;
    unsigned allocsz;
    unsigned allocsz_rounded;
    int fd_event;
    bool auto_restart;
    void (*on_buffer)(void*, struct buffer_discriptor*);
    void* on_buffer_param;
    struct buffer_discriptor bd[1];
    const void* frame;
    unsigned posts;
    unsigned posted_size;
    unsigned releases;
    unsigned closes;
};

typedef struct {
    unsigned gdev;
    struct buffers rx_strms[1];
    struct buffers tx_strms[1];
    unsigned rx_buffer_missed[1];
    unsigned app_drops[1];
} usb_dev_t;

static void* buffers_get_ptr(struct buffers* buffers, unsigned index)
{
    (void)index;
    return (void*)buffers->frame;
}

static int buffers_ready_post(struct buffers* buffers)
{
    (void)buffers;
    return 0;
}

static int buffers_usb_init(unsigned* gdev, struct buffers* buffers, unsigned requests,
                            unsigned count, unsigned block, unsigned endpoint, bool eventtype)
{
    (void)gdev;
    (void)requests;
    (void)endpoint;
    buffers->buf_max = count;
    buffers->allocsz = block;
    buffers->allocsz_rounded = (block + 4095) & ~4095u;
    buffers->fd_event = eventtype ? 1 : -1;
    return 0;
}

static unsigned _buffers_prod_get_nolock(struct buffers* buffers)
{
    (void)buffers;
    return 0;
}

static int buffers_usb_transfer_post(struct buffers* buffers, unsigned index,
                                    unsigned length, unsigned transfer)
{
    (void)index;
    (void)transfer;
    buffers->posts++;
    buffers->posted_size = length;
    return 0;
}

static int buffers_ready_wait(struct buffers* buffers, int64_t timeout_us)
{
    (void)timeout_us;
    return buffers->frame == NULL ? -ETIMEDOUT : 0;
}

static unsigned buffers_consume(struct buffers* buffers)
{
    (void)buffers;
    return 0;
}

static unsigned buffers_available_post(struct buffers* buffers)
{
    return buffers->releases++;
}

static int buffers_usb_free(struct buffers* buffers)
{
    buffers->closes++;
    return 0;
}

#include "../../../../vendor/libusdr/reference/usb_rx.inc"

// `events`: initial posts, rounded capacity, MTU, channel, receive errno,
// releases, closes. Output and OOB storage are valid for block bytes and 16 bytes.
int rsl_oracle_usb_rx(unsigned block, const unsigned char* frame, unsigned frame_size,
                      unsigned char* output, uint64_t* oob, unsigned* events)
{
    usb_dev_t dev = {0};
    lowlevel_stream_params_t params = {0};
    params.block_size = block;
    params.buffer_count = 32;
    stream_t channel = 99;
    int res = _usb_uram_init_rxstream(&dev, &params, &channel);
    if (res)
        return res;

    struct buffers* buffers = &dev.rx_strms[0];
    buffers->frame = frame;
    buffers->bd[0].b = buffers;
    buffers->bd[0].buffer_sz = frame_size;
    events[0] = buffers->posts;
    events[1] = buffers->posted_size;
    events[2] = params.out_mtu_size;
    events[3] = channel;

    void* payload = NULL;
    unsigned oob_size = 16;
    res = usb_uram_recv_dma_wait((lldev_t)&dev, 0, channel, &payload, oob, &oob_size, 1);
    events[4] = (unsigned)res;
    if (res == 0) {
        // Copy only bytes actually present: callers use valid full blocks for parity.
        unsigned size = frame_size - RX_PKT_TRAILER_EX;
        memcpy(output, payload, size < block ? size : block);
        res = usb_uram_recv_dma_release((lldev_t)&dev, 0, channel, payload);
        if (res)
            return res;
    }
    res = usb_uram_stream_deinitialize((lldev_t)&dev, 0, channel);
    events[5] = buffers->releases;
    events[6] = buffers->closes;
    return res;
}
