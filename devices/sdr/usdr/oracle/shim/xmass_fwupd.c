// libusdr's XMASS switch firmware tool (tools/xmass_fwupd.c, vendored unmodified) with its
// device calls redirected to a fake /dev/xmassN in Rust (src/xmass.rs).
//
// The tool's own headers are included first, so their include guards keep the macros
// below out of every declaration: the macros rename only the tool's calls to open, ioctl
// and close, and its main. Its file I/O stays real.

#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

// Implemented in src/xmass.rs. Each returns what the system call would, or a negative errno.
int rsl_oracle_xmass_open(const char* path, int flags);
int rsl_oracle_xmass_ioctl(int fd, unsigned long request, void* arg);
int rsl_oracle_xmass_close(int fd);

// As libc's: -1 with errno set on failure.
static int fake_result(int res)
{
    if (res < 0) {
        errno = -res;
        return -1;
    }
    return res;
}

static int fake_open(const char* path, int flags, ...)
{
    return fake_result(rsl_oracle_xmass_open(path, flags));
}

// glibc's ioctl reads its third argument as a pointer too, whatever the caller passed.
static int fake_ioctl(int fd, unsigned long request, ...)
{
    va_list ap;
    va_start(ap, request);
    void* arg = va_arg(ap, void*);
    va_end(ap);
    return fake_result(rsl_oracle_xmass_ioctl(fd, request, arg));
}

static int fake_close(int fd)
{
    return fake_result(rsl_oracle_xmass_close(fd));
}

#define open fake_open
#define ioctl fake_ioctl
#define close fake_close
#define main rsl_oracle_xmass_fwupd_main
#include "../../../../vendor/libusdr/tools/xmass_fwupd.c"
#undef main
#undef close
#undef ioctl
#undef open

enum { RSL_ORACLE_TOOL_SETUP_FAILED = -1 };

// Runs the tool with `argv`; what it prints to stdout and stderr goes, interleaved, to a
// buffer the caller frees. Returns its exit status, or RSL_ORACLE_TOOL_SETUP_FAILED with
// no log if the buffer cannot be set up.
int rsl_oracle_xmass_fwupd(int argc, char** argv, char** log, size_t* log_len)
{
    FILE* out = open_memstream(log, log_len);
    if (out == NULL) {
        *log = NULL;
        return RSL_ORACLE_TOOL_SETUP_FAILED;
    }
    fflush(stdout);
    fflush(stderr);
    FILE* saved_out = stdout;
    FILE* saved_err = stderr;
    stdout = out;
    stderr = out;
    int status = rsl_oracle_xmass_fwupd_main(argc, argv);
    stdout = saved_out;
    stderr = saved_err;
    fclose(out);
    return status;
}
