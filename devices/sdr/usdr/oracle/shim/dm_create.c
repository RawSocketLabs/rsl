// usdr_dm_create's TX sources over a buffer boundary: the vendored tool compiled whole, its
// `main` renamed, so its own producer threads (the tone, chirp, table and float-tone
// generators and the file reader) fill its own ring buffer, and its own `do_transmit`
// sends from it. The oracle takes the packets out of the ring, or hands `do_transmit` a TX
// stream it created on the simulated board.
#define main rsl_dm_create_main
#include "../libusdr/tools/usdr_dm_create.c"
#undef main

#include <errno.h>
#include <pthread.h>

// Which producer thread the tool would start.
enum rsl_gen_kind {
    // freq_gen_thread_ci16: the tone, or the chirp when `chirp_steps` is nonzero.
    RSL_GEN_CI16 = 0,
    // freq_gen_thread_ci16_lut (`-J`).
    RSL_GEN_TABLE = 1,
    // freq_gen_thread_cf32.
    RSL_GEN_CF32 = 2,
    // disk_read_thread (`-I`, with `-o` to cycle).
    RSL_GEN_FILE = 3,
};

// One channel's producer settings: tx_thread_input_t's fields, plus the file mode's.
struct rsl_gen {
    int kind;
    unsigned samplerate;
    unsigned samples_count;
    float gain;
    double start_phase;
    double delta_phase;
    double chirp_freq0;
    double chirp_freq1;
    int32_t chirp_steps;
    // File mode: the file's bytes, whether to cycle, and the read block in bytes.
    const void* file;
    size_t file_len;
    bool cycle;
    unsigned block;
};

// The tool's phase conversion, as its generators write it.
int32_t rsl_oracle_dm_create_phase(double turns)
{
    int32_t phase = WVLT_CONVPHASE_F32_I32(turns);
    return phase;
}

// The tool's `ci16` gain, as its generators write it.
int16_t rsl_oracle_dm_create_gain(float dbfs)
{
    const int16_t gain = DBFS_TO_AMPLITUDE(dbfs, MAX_TXGEN_CI16_AMPL);
    return gain;
}

// Packets the ring holds; the tool's 256 only lets its producers run further ahead.
#define RSL_RING_ITEMS 8

// Starts channel 0's producer for `g` on a ring of `payload`-byte packets.
static int rsl_start(const struct rsl_gen* g, unsigned payload, pthread_t* thread)
{
    tx_thread_input_t* inp = &tx_thread_inputs[0];
    memset(inp, 0, sizeof(*inp));
    inp->chan = 0;
    inp->samplerate = g->samplerate;
    inp->samples_count = g->samples_count;
    inp->gain = g->gain;
    inp->start_phase = g->start_phase;
    inp->delta_phase = g->delta_phase;
    inp->chirp_freq0 = g->chirp_freq0;
    inp->chirp_freq1 = g->chirp_freq1;
    inp->chirp_steps = g->chirp_steps;

    fn_rxtx_thread_t thread_func;
    switch (g->kind) {
    case RSL_GEN_CI16: thread_func = freq_gen_thread_ci16; break;
    case RSL_GEN_TABLE: thread_func = freq_gen_thread_ci16_lut; break;
    case RSL_GEN_CF32: thread_func = freq_gen_thread_cf32; break;
    case RSL_GEN_FILE: {
        FILE* f = tmpfile();
        if (!f)
            return -errno;
        if (g->file_len && fwrite(g->file, 1, g->file_len, f) != g->file_len) {
            fclose(f);
            return -EIO;
        }
        rewind(f);
        s_in_file[0] = f;
        s_tx_blksz = g->block;
        tx_file_cycle = g->cycle;
        thread_func = disk_read_thread;
        break;
    }
    default: return -EINVAL;
    }

    tbuff[0] = ring_buffer_create(RSL_RING_ITEMS, sizeof(tx_header_t) + payload);
    if (!tbuff[0])
        return -ENOMEM;
    s_stop = false;
    thread_stop = false;
    int res = pthread_create(thread, NULL, thread_func, inp);
    if (res) {
        ring_buffer_destroy(tbuff[0]);
        return -res;
    }
    return 0;
}

// Stops the producer as the tool does (`thread_stop`) and releases what rsl_start took.
static void rsl_stop(pthread_t thread)
{
    thread_stop = true;
    pthread_join(thread, NULL);
    thread_stop = false;
    ring_buffer_destroy(tbuff[0]);
    tbuff[0] = NULL;
    if (s_in_file[0]) {
        fclose(s_in_file[0]);
        s_in_file[0] = NULL;
    }
}

// Takes up to `packets` packets from channel 0's producer, stopping after one that flags
// the end of its file: each packet's payload goes to `out + k * payload`, its header's
// length and flags to `lens[k]` and `flags[k]`. Returns the packets taken, or a negative
// errno.
int rsl_oracle_dm_create_generate(const struct rsl_gen* g, unsigned payload, unsigned packets,
                                  uint8_t* out, uint32_t* lens, uint32_t* flags)
{
    pthread_t thread;
    int res = rsl_start(g, payload, &thread);
    if (res)
        return res;

    unsigned k = 0;
    while (k < packets) {
        unsigned idx = ring_buffer_cwait(tbuff[0], 1000000);
        if (idx == IDX_TIMEDOUT) {
            res = -ETIMEDOUT;
            break;
        }
        const char* data = ring_buffer_at(tbuff[0], idx);
        const tx_header_t* hdr = (const tx_header_t*)data;
        if (hdr->len > payload) {
            res = -EOVERFLOW;
            break;
        }
        memcpy(out + (size_t)k * payload, data + sizeof(tx_header_t), hdr->len);
        lens[k] = hdr->len;
        flags[k] = hdr->flags;
        ring_buffer_cpost(tbuff[0]);
        k++;
        if (hdr->flags)
            break;
    }

    rsl_stop(thread);
    return res ? res : (int)k;
}

// The tool's TX-only loop on `stream` for up to `iterations` iterations: channel 0's
// producer for `g`, then `do_transmit` each iteration from the tool's first time (one
// packet's samples, `start_tx_delay`), or "now" with `nots`. `olens[i]` gets the samples
// iteration `i` sent. Returns the iterations run, or a negative errno; `*stopped` is set
// when the last one's `do_transmit` ended the loop (after a file's end or a failed send).
int rsl_oracle_dm_create_transmit(pusdr_dms_t stream, const struct rsl_gen* g,
                                  unsigned iterations, bool nots, unsigned* olens,
                                  bool* stopped)
{
    usdr_dms_nfo_t nfo;
    int res = usdr_dms_info(stream, &nfo);
    if (res)
        return res;

    pthread_t thread;
    tx_bufcnt = 1;
    res = rsl_start(g, nfo.pktbszie, &thread);
    if (res)
        return res;

    uint64_t stm = g->samples_count;
    usdr_dms_send_stat_t st;
    unsigned i = 0;
    *stopped = false;
    while (i < iterations && !*stopped) {
        olens[i] = 0;
        *stopped = !do_transmit(stream, &stm, &nfo, nots, i, &olens[i], &st);
        i++;
    }

    rsl_stop(thread);
    tx_bufcnt = 0;
    return (int)i;
}
