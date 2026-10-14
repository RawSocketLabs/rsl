// usdr_dm_gpsdo's discipline loop over a measurement and DAC boundary: the vendored tool
// compiled whole, its `main` renamed, with these calls redirected before it is included.
//
// - The measurement read (`/dm/sensor/freqpps`, which the uSDR does not have at the PIN)
//   loads the oracle's next scripted word into the board, then reads the tool's own
//   commented-out alternative, `/dm/sdr/0/clkmeas` (GPI word 24), through libusdr. With
//   the script spent it fails, which ends the tool's loop.
// - The DAC write (`/dm/sdr/0/dac_vctcxo`) goes to libusdr unchanged.
// - The set-up before the loop (reference, rate, RX stream, sync and start) is recorded,
//   not performed: the port leaves it to its caller.
// - The 100 ms timer and the monotonic clock are a virtual clock the timer's every poll
//   advances by 100 ms, so holdover is deterministic.
// - Its printed lines are recorded with their values, not printed; logging is left alone.
//
// Everything between, the control arithmetic included, is the tool's own code.
#define _GNU_SOURCE
#include <errno.h>
#include <getopt.h>
#include <inttypes.h>
#include <math.h>
#include <poll.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/timerfd.h>
#include <time.h>
#include <unistd.h>

#include <dm_dev.h>
#include <dm_rate.h>
#include <dm_stream.h>
#include <usdr_logging.h>

// What the tool did, in order.
enum rsl_gpsdo_kind {
    // A measurement read began; `mark` is the board's trace length then.
    RSL_GPSDO_READ = 1,
    // "PPS=...": the word, its count, the measured frequency and the error in ppm.
    RSL_GPSDO_PPS = 2,
    // "Error more than 2%, skip iteration".
    RSL_GPSDO_SKIP = 3,
    // "POUT=...": the PID output, the correction in Hz and the steps asked for.
    RSL_GPSDO_POUT = 4,
    // "DAC value: ...": the new code, the DAC's bits, the code before and the step.
    RSL_GPSDO_DAC = 5,
    // The DAC write and libusdr's result.
    RSL_GPSDO_DAC_WRITE = 6,
    // "No PPS trigger for more than ... seconds - entering holdover mode".
    RSL_GPSDO_HOLDOVER = 7,
    // "Found PPS trigger".
    RSL_GPSDO_FOUND = 8,
    // A set-up call; `i0` says which (`enum rsl_gpsdo_setup`), `word` its value.
    RSL_GPSDO_SETUP = 9,
    // A line on stderr (an error the tool reports).
    RSL_GPSDO_STDERR = 10,
};

// The set-up calls, in the tool's order.
enum rsl_gpsdo_setup {
    RSL_SETUP_REFCLK_FREQUENCY = 1,
    RSL_SETUP_REFCLK_PATH = 2,
    RSL_SETUP_RATE = 3,
    RSL_SETUP_RX_STREAM = 4,
    RSL_SETUP_SYNC = 5,
    RSL_SETUP_START = 6,
};

struct rsl_gpsdo_event {
    int32_t kind;
    int32_t i0;
    int32_t i1;
    int32_t i2;
    uint64_t word;
    double a;
    double b;
    double c;
};

// The oracle: loads the next scripted word into the board (0), or reports the script spent.
int rsl_oracle_gpsdo_next_measurement(void);
// The oracle: the board's trace length.
size_t rsl_oracle_gpsdo_trace_mark(void);

static struct rsl_gpsdo_event* s_events;
static size_t s_capacity;
static size_t s_count;
static struct timespec s_now;
// Stands in for the RX stream the tool creates.
static int s_fake_stream;
// The code the tool's DAC write last succeeded with in this process: what `set_dac_value`'s
// function `static`, which nothing resets between runs, holds (0 before any write).
static uint64_t s_last_dac;

#define RSL_FAKE_TIMER_FD 0x7fff

static struct rsl_gpsdo_event* rsl_push(int32_t kind)
{
    static struct rsl_gpsdo_event overflow;
    if (s_count == s_capacity) {
        // Counted, so the oracle sees it ran out of room.
        s_count++;
        return &overflow;
    }
    if (s_count > s_capacity)
        return &overflow;
    struct rsl_gpsdo_event* e = &s_events[s_count++];
    memset(e, 0, sizeof(*e));
    e->kind = kind;
    return e;
}

static void rsl_setup(int32_t which, uint64_t value)
{
    struct rsl_gpsdo_event* e = rsl_push(RSL_GPSDO_SETUP);
    e->i0 = which;
    e->word = value;
}

static int rsl_gpsdo_get_uint(pdm_dev_t dev, const char* path, uint64_t* oval)
{
    (void)path;
    rsl_push(RSL_GPSDO_READ)->word = rsl_oracle_gpsdo_trace_mark();
    if (rsl_oracle_gpsdo_next_measurement())
        return -ENODATA;
    return usdr_dme_get_uint(dev, "/dm/sdr/0/clkmeas", oval);
}

static int rsl_gpsdo_set_uint(pdm_dev_t dev, const char* path, uint64_t val)
{
    if (strcmp(path, "/dm/sdr/0/dac_vctcxo") == 0) {
        int res = usdr_dme_set_uint(dev, path, val);
        if (res == 0)
            s_last_dac = val;
        struct rsl_gpsdo_event* e = rsl_push(RSL_GPSDO_DAC_WRITE);
        e->word = val;
        e->i0 = res;
        return res;
    }
    if (strcmp(path, "/dm/sdr/refclk/frequency") == 0) {
        rsl_setup(RSL_SETUP_REFCLK_FREQUENCY, val);
    } else if (strcmp(path, "/dm/sdr/refclk/path") == 0) {
        // The tool passes the path name's address as the value.
        rsl_setup(RSL_SETUP_REFCLK_PATH, strcmp((const char*)(uintptr_t)val, "external") == 0);
    } else {
        return -ENOENT;
    }
    return 0;
}

static int rsl_gpsdo_rate_set(pdm_dev_t dev, const char* rate_name, unsigned rate)
{
    (void)dev;
    (void)rate_name;
    rsl_setup(RSL_SETUP_RATE, rate);
    return 0;
}

static int rsl_gpsdo_dms_create(pdm_dev_t device, const char* sobj, const char* dformat,
                                logical_ch_msk_t channels, unsigned pktsyms, pusdr_dms_t* outu)
{
    (void)device;
    (void)sobj;
    (void)dformat;
    (void)channels;
    rsl_setup(RSL_SETUP_RX_STREAM, pktsyms);
    *outu = (pusdr_dms_t)&s_fake_stream;
    return 0;
}

static int rsl_gpsdo_dms_sync(pdm_dev_t device, const char* synctype, unsigned scount,
                              pusdr_dms_t* pstream)
{
    (void)device;
    (void)pstream;
    rsl_setup(RSL_SETUP_SYNC, strcmp(synctype, "any") == 0 ? scount : 0);
    return 0;
}

static int rsl_gpsdo_dms_op(pusdr_dms_t stream, unsigned command, dm_time_t tm)
{
    (void)stream;
    (void)tm;
    rsl_setup(RSL_SETUP_START, command);
    return 0;
}

static void rsl_gpsdo_log_setlevel(const char* subsystem, unsigned loglevel)
{
    (void)subsystem;
    (void)loglevel;
}

static void rsl_gpsdo_log_colorize(const char* subsystem)
{
    (void)subsystem;
}

static int rsl_gpsdo_timerfd_create(int clockid, int flags)
{
    (void)clockid;
    (void)flags;
    return RSL_FAKE_TIMER_FD;
}

static int rsl_gpsdo_timerfd_settime(int fd, int flags, const struct itimerspec* new_value,
                                     struct itimerspec* old_value)
{
    (void)fd;
    (void)flags;
    (void)new_value;
    (void)old_value;
    return 0;
}

// One timer expiry: the virtual clock moves on 100 ms.
static int rsl_gpsdo_poll(struct pollfd* fds, nfds_t nfds, int timeout)
{
    (void)timeout;
    s_now.tv_nsec += 100000000;
    if (s_now.tv_nsec >= 1000000000) {
        s_now.tv_nsec -= 1000000000;
        s_now.tv_sec++;
    }
    for (nfds_t i = 0; i < nfds; i++)
        fds[i].revents = POLLIN;
    return (int)nfds;
}

static ssize_t rsl_gpsdo_read(int fd, void* buf, size_t count)
{
    (void)fd;
    uint64_t expirations = 1;
    memcpy(buf, &expirations, count < sizeof(expirations) ? count : sizeof(expirations));
    return (ssize_t)count;
}

static int rsl_gpsdo_close(int fd)
{
    (void)fd;
    return 0;
}

static int rsl_gpsdo_clock_gettime(clockid_t clockid, struct timespec* tp)
{
    (void)clockid;
    *tp = s_now;
    return 0;
}

static int rsl_gpsdo_printf(const char* format, ...)
{
    va_list ap;
    va_start(ap, format);
    struct rsl_gpsdo_event* e;
    if (strncmp(format, "PPS=", 4) == 0) {
        e = rsl_push(RSL_GPSDO_PPS);
        e->word = va_arg(ap, uint64_t);
        e->i0 = va_arg(ap, int);
        e->a = va_arg(ap, double);
        e->b = va_arg(ap, double);
    } else if (strncmp(format, "POUT=", 5) == 0) {
        e = rsl_push(RSL_GPSDO_POUT);
        e->a = va_arg(ap, double);
        e->b = va_arg(ap, double);
        e->c = va_arg(ap, double);
    } else if (strncmp(format, "DAC value:", 10) == 0) {
        e = rsl_push(RSL_GPSDO_DAC);
        e->i0 = (int32_t)va_arg(ap, unsigned);
        e->i1 = (int32_t)va_arg(ap, unsigned);
        e->i2 = (int32_t)va_arg(ap, unsigned);
        e->a = va_arg(ap, double);
    } else if (strncmp(format, "Error more", 10) == 0) {
        rsl_push(RSL_GPSDO_SKIP);
    } else if (strncmp(format, "No PPS", 6) == 0) {
        rsl_push(RSL_GPSDO_HOLDOVER)->a = va_arg(ap, double);
    } else if (strncmp(format, "Found PPS", 9) == 0) {
        rsl_push(RSL_GPSDO_FOUND);
    } else {
        // A line this shim does not know: the oracle fails on kind 0.
        rsl_push(0);
    }
    va_end(ap);
    return 0;
}

static int rsl_gpsdo_fprintf(FILE* stream, const char* format, ...)
{
    (void)stream;
    (void)format;
    rsl_push(RSL_GPSDO_STDERR);
    return 0;
}

#define usdr_dme_get_uint rsl_gpsdo_get_uint
#define usdr_dme_set_uint rsl_gpsdo_set_uint
#define usdr_dmr_rate_set rsl_gpsdo_rate_set
#define usdr_dms_create rsl_gpsdo_dms_create
#define usdr_dms_sync rsl_gpsdo_dms_sync
#define usdr_dms_op rsl_gpsdo_dms_op
#define usdrlog_setlevel rsl_gpsdo_log_setlevel
#define usdrlog_enablecolorize rsl_gpsdo_log_colorize
#define timerfd_create rsl_gpsdo_timerfd_create
#define timerfd_settime rsl_gpsdo_timerfd_settime
#define poll rsl_gpsdo_poll
#define read rsl_gpsdo_read
#define close rsl_gpsdo_close
#define clock_gettime rsl_gpsdo_clock_gettime
#define printf rsl_gpsdo_printf
#define fprintf rsl_gpsdo_fprintf
#define main rsl_gpsdo_main
#include "../../../../vendor/libusdr/tools/usdr_dm_gpsdo.c"
#undef main

// Runs the tool with `argc`/`argv` (its options), recording into `events` (room for
// `capacity`; `*count` may exceed it, meaning events were lost). `*dac_before` gets the
// code `set_dac_value` will skip writing first. The virtual clock starts at 1000 s.
// Returns the tool's exit status.
int rsl_oracle_gpsdo_run(int argc, char** argv, struct rsl_gpsdo_event* events,
                         size_t capacity, size_t* count, uint64_t* dac_before)
{
    *dac_before = s_last_dac;
    s_events = events;
    s_capacity = capacity;
    s_count = 0;
    s_now.tv_sec = 1000;
    s_now.tv_nsec = 0;
    // getopt's state is process-global: start it over.
    optind = 0;
    int status = rsl_gpsdo_main(argc, argv);
    *count = s_count;
    s_events = NULL;
    return status;
}
