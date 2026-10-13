// Calls libusdr's LMK05318 planning functions (hw/lmk05318/lmk05318.c) on a state the
// caller supplies and captures the registers each stages, for rsl-usdr's plan parity tests.
//
// The register map is static in lmk05318.c; each call empties it first, then writes it out
// (lmk05318_reg_wr_from_map) to a capture device that records every register write by
// address. Calls must not overlap: the Rust side holds the oracle's session lock.

#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <string.h>

#include "usdr_lowlevel.h"
#include "hw/lmk05318/lmk05318.h"

// Exported by lmk05318.c without a prototype in its header.
void lmk05318_registers_map_reset(void);

// One capture slot per staged address, 0x0000 to 0x019B: 0x100 | value once written.
enum { RSL_LMK_MAP_LEN = 0x19c };

static uint16_t* s_capture;

static int capture_ls_op(lldev_t dev, subdev_t subdev, unsigned op, lsopaddr_t addr,
                         size_t meminsz, void* pin, size_t memoutsz, const void* pout)
{
    (void)dev;
    (void)subdev;
    (void)addr;
    (void)pin;
    if (op != USDR_LSOP_I2C_DEV || meminsz != 0 || memoutsz != 3 || s_capture == NULL) {
        return -EINVAL;
    }
    const uint8_t* bytes = (const uint8_t*)pout;
    unsigned reg = (unsigned)bytes[0] << 8 | bytes[1];
    if (reg >= RSL_LMK_MAP_LEN) {
        return -EINVAL;
    }
    s_capture[reg] = 0x100 | bytes[2];
    return 0;
}

static lowlevel_ops_t s_capture_ops = { .ls_op = capture_ls_op };
static lowlevel_dev_t s_capture_dev = { .ops = &s_capture_ops };

// Writes the staged map into `regs` (RSL_LMK_MAP_LEN entries) and empties it.
static void capture_map(lmk05318_state_t* st, uint16_t* regs)
{
    s_capture = regs;
    lmk05318_reg_wr_from_map(st, false);
    s_capture = NULL;
}

// One output request in, its result out; `port` and `freq` come back in libusdr's sorted
// order.
struct rsl_lmk_port {
    uint32_t port;
    uint32_t freq;
    int32_t revert;
    int32_t type;
    int32_t affinity;
    int32_t mux;
    uint64_t out_div;
    double freq_out;
};

// APLL2's state, in and out.
struct rsl_lmk_apll2 {
    uint32_t rp;
    uint32_t rs;
    uint64_t vco2_freq;
    uint32_t n;
    uint32_t num;
    uint32_t den;
    uint32_t pd1;
    uint32_t pd2;
};

// lmk05318_set_xo_fref (its result returned), then lmk05318_tune_apll1 without a DPLL
// (its result in `apll1_res`) whenever the first chose an XO path, even if it then found no
// BAW settings for the XO: APLL1's maths depend only on the path.
int rsl_oracle_lmk05318_xo_apll1(uint32_t fref, int32_t type, int32_t fdet_bypass,
                                 uint32_t* doubler, uint32_t* rdiv, int32_t* apll1_res,
                                 uint16_t* regs)
{
    lmk05318_state_t st;
    memset(&st, 0, sizeof(st));
    st.dev = &s_capture_dev;
    st.xo.fref = fref;
    st.xo.type = (xo_input_type_t)type;
    st.xo.fdet_bypass = fdet_bypass != 0;
    lmk05318_registers_map_reset();
    int res = lmk05318_set_xo_fref(&st);
    *apll1_res = st.xo.pll1_fref_rdiv ? lmk05318_tune_apll1(&st) : -EINVAL;
    *doubler = st.xo.doubler_enabled;
    *rdiv = st.xo.pll1_fref_rdiv;
    capture_map(&st, regs);
    return res;
}

// lmk05318_solver on APLL2's state `apll2` for `count` requests.
int rsl_oracle_lmk05318_solve(struct rsl_lmk_apll2* apll2, struct rsl_lmk_port* ports,
                              unsigned count, uint16_t* regs)
{
    if (count > LMK05318_MAX_OUT_PORTS) {
        return -EINVAL;
    }
    lmk05318_state_t st;
    memset(&st, 0, sizeof(st));
    st.dev = &s_capture_dev;
    st.fref_pll2_div_rp = apll2->rp;
    st.fref_pll2_div_rs = apll2->rs;
    st.vco2_freq = apll2->vco2_freq;
    st.vco2_n = apll2->n;
    st.vco2_num = apll2->num;
    st.vco2_den = apll2->den;
    st.pd1 = apll2->pd1;
    st.pd2 = apll2->pd2;

    lmk05318_out_config_t outs[LMK05318_MAX_OUT_PORTS];
    for (unsigned i = 0; i < count; i++) {
        lmk05318_port_request(&outs[i], ports[i].port, ports[i].freq, ports[i].revert != 0,
                              (lmk05318_type_t)ports[i].type);
        lmk05318_set_port_affinity(&outs[i], (lmk05318_port_affinity_t)ports[i].affinity);
    }
    lmk05318_registers_map_reset();
    int res = lmk05318_solver(&st, outs, count);

    apll2->vco2_freq = st.vco2_freq;
    apll2->n = st.vco2_n;
    apll2->num = st.vco2_num;
    apll2->den = st.vco2_den;
    apll2->pd1 = st.pd1;
    apll2->pd2 = st.pd2;
    for (unsigned i = 0; i < count; i++) {
        ports[i].port = outs[i].port;
        ports[i].freq = outs[i].wanted.freq;
        ports[i].mux = outs[i].result.mux;
        ports[i].out_div = outs[i].result.out_div;
        ports[i].freq_out = outs[i].result.freq;
    }
    capture_map(&st, regs);
    return res;
}

// lmk05318_set_out_div for one output.
int rsl_oracle_lmk05318_out_div(uint32_t port, uint64_t div, uint16_t* regs)
{
    lmk05318_state_t st;
    memset(&st, 0, sizeof(st));
    st.dev = &s_capture_dev;
    lmk05318_registers_map_reset();
    int res = lmk05318_set_out_div(&st, port, div);
    capture_map(&st, regs);
    return res;
}
