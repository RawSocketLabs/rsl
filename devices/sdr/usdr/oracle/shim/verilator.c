// libusdr's Verilator host bridge (lowlevel/verilator_ll), compiled whole from the vendored
// source and driven against a fake model the tests serve (usdr's
// tests/common/verilator_peer.rs).
//
// At the PIN the bridge no longer builds against the rest of libusdr (upstream only builds it
// with ENABLE_VERILATOR, off by default): three calls predate the current device API, and
// its plugin's ops table and device pointers mistype. This file includes it unchanged and
// adapts only those three calls by macro, each below at its definition; build.rs downgrades
// the pointer-type mismatches, all between pointers to the same object, from GCC's errors to
// nothing. vpu.c and unix_vll.c, the wire framing and the socket and shared-file calls, are
// compiled as they are, beside this file.
//
// Two harness measures, neither part of the bridge's behaviour on the wire:
// - The model's socket and shared file are found in a directory the test names, where the
//   bridge opens "verilator.sock" and "verilator.sock.mmap" in the working directory.
// - The bridge's state is allocated zeroed. The plugin mallocs it and initialises only
//   some fields: the notification index, the RX buffer count, the DMA status write index,
//   the delayed-interrupt count, the monitor thread's stop flag and the SPI/I2C readback
//   words are left as the allocator returns them. Zero is the state the bridge's logic
//   assumes (its ring starts at entry 0 in phase 0, its buffers at 0), and libusdr's device
//   create asserts the device pointer starts null.

#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>

// Every header the bridge includes, so the macros below reach only the bridge's own code.
#include "usdr_port.h"
#include "usdr_logging.h"
#include "device/device.h"
#include "device/device_bus.h"
#include "device/device_cores.h"
#include "device/device_names.h"
#include "device/device_vfs.h"
#include "ipblks/si2c.h"
#include "verilator_ll/unix_vll.h"
#include "verilator_ll/verilatorll_wrap.h"
#include "verilator_ll/vpu.h"

// The directory of the model's socket and shared file, for the bridge being opened on this
// thread (the bridge opens both from its create, on the caller's thread).
static __thread const char* s_model_dir;
static __thread char s_model_path[256];

// `name`, which the bridge opens relative to the working directory, inside the test's
// directory instead.
static const char* rsl_oracle_vll_path(const char* name)
{
    snprintf(s_model_path, sizeof(s_model_path), "%s/%s", s_model_dir, name);
    return s_model_path;
}

// The harness's two redirections and its zeroed allocation, as described above.
#define vll_chan_connect(vc, dev) vll_chan_connect((vc), rsl_oracle_vll_path(dev))
#define vll_mem_open(mm, mem, maxsz) vll_mem_open((mm), rsl_oracle_vll_path(mem), (maxsz))
#define malloc(size) calloc(1, (size))

// The API drift. `usdr_device_create(dev, did, &dev->udev)` now takes the transport handle
// alone and sets its `pdev`, which is the bridge device's `udev` (both follow `ops`).
#define usdr_device_create(dev, did, pudev) usdr_device_create((lldev_t)(dev), (did))
// `usdr_device_vfs_obj_val_get_by_path` is now `usdr_device_vfs_obj_val_get_u64`.
#define usdr_device_vfs_obj_val_get_by_path usdr_device_vfs_obj_val_get_u64
// `initialize` now takes the device's connection parameters too. The board bring-up it
// starts is libusdr's device code, not the bridge, and usdr's `Device` runs its own
// over any `Bus`, so the harness skips it: `dev->udev->initialize(dev->udev)` becomes a
// check that the device has one, then success.
#define initialize(udev) initialize == NULL ? -ENODEV : rsl_oracle_vll_no_bring_up(udev)

static int rsl_oracle_vll_no_bring_up(device_t* udev)
{
    (void)udev;
    return 0;
}

#include "verilator_ll/verilatorll_wrap.c"

#undef malloc
#undef initialize

// Connects a bridge to the model serving `dir`/verilator.sock, as libusdr's plugin create
// does (handshake, monitor thread, device description, interrupt set-up), board bring-up
// excepted. Returns 0 with `*out` set, or the create's error.
int rsl_oracle_vll_open(const char* dir, void** out)
{
    lldev_t dev = NULL;
    s_model_dir = dir;
    int res = verilator_wrap_plugin_create(0, NULL, NULL, &dev, 0, NULL, 0);
    s_model_dir = NULL;
    *out = dev;
    return res;
}

// `ls_op` on the bridge: 0 or a negative errno.
int rsl_oracle_vll_ls_op(void* dev, unsigned op, unsigned addr, size_t meminsz, void* pin,
                         size_t memoutsz, const void* pout)
{
    return verilator_wrap_ls_op(dev, 0, op, addr, meminsz, pin, memoutsz, pout);
}

// Sets RX stream 0 up as libusdr's stream code asks a transport to: `buffers` buffers of
// `block` bytes.
int rsl_oracle_vll_rx_open(void* dev, unsigned block, unsigned buffers)
{
    lowlevel_stream_params_t params;
    memset(&params, 0, sizeof(params));
    params.streamno = 0;
    params.block_size = block;
    params.buffer_count = buffers;
    stream_t channel = 0;
    return verilator_wrap_stream_initialize(dev, 0, &params, &channel);
}

// Waits for the next RX block, copies its first `len` bytes to `out`, and releases it, as
// libusdr's stream code does between the two calls. The bridge writes the two status words
// it has for the block through its `oob_size` argument, not `oob_ptr`, so `oob` stands in
// for that argument: it holds the size on the way in and the words on the way out. (Under
// libusdr's stream code they overwrite its size variable and the four bytes after it, and
// its out-of-band array stays unwritten.)
int rsl_oracle_vll_rx_recv(void* dev, void* out, size_t len, unsigned oob[2])
{
    void* buffer = NULL;
    oob[0] = 2 * sizeof(uint64_t); // libusdr's stream code offers 16 bytes
    oob[1] = 0;
    int res = verilator_wrap_recv_dma_wait(dev, 0, 0, &buffer, NULL, oob, 0);
    if (res) {
        return res;
    }
    memcpy(out, buffer, len);
    return verilator_wrap_recv_dma_release(dev, 0, 0, buffer);
}

// Tears a bridge down: the bridge has no destroy that stops its monitor thread (its
// destroy frees the state under it), so the harness ends the connection, which ends the
// thread's receive, joins it, then releases what the create made.
void rsl_oracle_vll_close(void* dev)
{
    verilator_dev_t* d = dev;
    d->terminated = true;
    shutdown(d->proto.vch.fd, SHUT_RDWR);
    pthread_join(d->iothread, NULL);
    vll_chan_close(&d->proto.vch);
    vll_mem_close(&d->proto.vchm);
    if (d->udev) {
        usdr_device_base_destroy(d->udev);
        free(d->udev);
    }
    free(d);
}
