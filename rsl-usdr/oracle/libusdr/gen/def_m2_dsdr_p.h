enum m2_dsdr_p_regs_t {
    REFCTRL = 0x10,
    CHA = 0x11,
    CHB = 0x12,
    CHC = 0x13,
    CHD = 0x14,
    ATT_RX_CHA = 0x15,
    ATT_RX_CHB = 0x16,
    ATT_RX_CHC = 0x17,
    ATT_RX_CHD = 0x18,
};
#define MAKE_M2_DSDR_P_REG_WR(a, v) (0x80000000 | ((a) << 24) | ((v) & 0xffffff))
#define MAKE_M2_DSDR_P_REG_RD(a) (((a) << 24))
enum m2_dsdr_p_tdd_fdd_opts_t {
    TDD_FDD_OPTS_REV0_LNA_TO_RX_____REV2_SHUTDOWN = 0x0,
    TDD_FDD_OPTS_REV0_LNA_TO_TDDSW__REV2_LNA_TO_TDDSW = 0x1,
    TDD_FDD_OPTS_REV0_LNA_TO_RX_____REV2_LNA_TO_LB = 0x2,
    TDD_FDD_OPTS_REV0_LNA_TO_TDDSW__REV2_LNA_TO_RX = 0x3,
};
// Register R16 [0x10] -- REFCTRL

enum refctrl_fields_t {
    REFCTRL_EXTERNAL_OFF = 0x0,
    REFCTRL_EXTERNAL_MSK = 0x1,
};
#define GET_M2_DSDR_P_REFCTRL_EXTERNAL(x) (((x) & REFCTRL_EXTERNAL_MSK) >> REFCTRL_EXTERNAL_OFF)
#define SET_M2_DSDR_P_REFCTRL_EXTERNAL(p, f) (p) = ((p) & ~REFCTRL_EXTERNAL_MSK) | (((f) << REFCTRL_EXTERNAL_OFF) & REFCTRL_EXTERNAL_MSK)

#define MAKE_M2_DSDR_P_REFCTRL(external) MAKE_M2_DSDR_P_REG_WR(REFCTRL, \
    (((external) << REFCTRL_EXTERNAL_OFF) & REFCTRL_EXTERNAL_MSK))
// Register R17 [0x11] -- CHA
enum cha_sw_rx_tddfdd_options {
    CHA_SW_RX_TDDFDD_REV0_LNA_TO_RX_____REV2_SHUTDOWN = 0,
    CHA_SW_RX_TDDFDD_REV0_LNA_TO_TDDSW__REV2_LNA_TO_TDDSW = 1,
    CHA_SW_RX_TDDFDD_REV0_LNA_TO_RX_____REV2_LNA_TO_LB = 2,
    CHA_SW_RX_TDDFDD_REV0_LNA_TO_TDDSW__REV2_LNA_TO_RX = 3,
};

enum cha_fields_t {
    CHA_EN_TX_OFF = 0x7,
    CHA_EN_TX_MSK = 0x80,
    CHA_EN_VADJ_OFF = 0x6,
    CHA_EN_VADJ_MSK = 0x40,
    CHA_EN_RX_OFF = 0x5,
    CHA_EN_RX_MSK = 0x20,
    CHA_SW_PA_ONOFF_OFF = 0x2,
    CHA_SW_PA_ONOFF_MSK = 0x4,
    CHA_SW_RX_TDDFDD_OFF = 0x1,
    CHA_SW_RX_TDDFDD_MSK = 0xa,
    CHA_SW_RXTX_OFF = 0x0,
    CHA_SW_RXTX_MSK = 0x1,
};
#define GET_M2_DSDR_P_CHA_EN_TX(x) (((x) & CHA_EN_TX_MSK) >> CHA_EN_TX_OFF)
#define GET_M2_DSDR_P_CHA_EN_VADJ(x) (((x) & CHA_EN_VADJ_MSK) >> CHA_EN_VADJ_OFF)
#define GET_M2_DSDR_P_CHA_EN_RX(x) (((x) & CHA_EN_RX_MSK) >> CHA_EN_RX_OFF)
#define GET_M2_DSDR_P_CHA_SW_PA_ONOFF(x) (((x) & CHA_SW_PA_ONOFF_MSK) >> CHA_SW_PA_ONOFF_OFF)
#define GET_M2_DSDR_P_CHA_SW_RX_TDDFDD(x) ((((((x) & CHA_SW_RX_TDDFDD_MSK) >> 1) & 0x1) << 0) | (((((x) & CHA_SW_RX_TDDFDD_MSK) >> 3) & 0x1) << 1))
#define GET_M2_DSDR_P_CHA_SW_RXTX(x) (((x) & CHA_SW_RXTX_MSK) >> CHA_SW_RXTX_OFF)
#define SET_M2_DSDR_P_CHA_EN_TX(p, f) (p) = ((p) & ~CHA_EN_TX_MSK) | (((f) << CHA_EN_TX_OFF) & CHA_EN_TX_MSK)
#define SET_M2_DSDR_P_CHA_EN_VADJ(p, f) (p) = ((p) & ~CHA_EN_VADJ_MSK) | (((f) << CHA_EN_VADJ_OFF) & CHA_EN_VADJ_MSK)
#define SET_M2_DSDR_P_CHA_EN_RX(p, f) (p) = ((p) & ~CHA_EN_RX_MSK) | (((f) << CHA_EN_RX_OFF) & CHA_EN_RX_MSK)
#define SET_M2_DSDR_P_CHA_SW_PA_ONOFF(p, f) (p) = ((p) & ~CHA_SW_PA_ONOFF_MSK) | (((f) << CHA_SW_PA_ONOFF_OFF) & CHA_SW_PA_ONOFF_MSK)
#define SET_M2_DSDR_P_CHA_SW_RX_TDDFDD(p, f) (p) = ((p) & ~CHA_SW_RX_TDDFDD_MSK) | ((((((f) >> 1) & 0x1) << 3) | ((((f) >> 0) & 0x1) << 1)) & CHA_SW_RX_TDDFDD_MSK)
#define SET_M2_DSDR_P_CHA_SW_RXTX(p, f) (p) = ((p) & ~CHA_SW_RXTX_MSK) | (((f) << CHA_SW_RXTX_OFF) & CHA_SW_RXTX_MSK)

#define MAKE_M2_DSDR_P_CHA(en_tx, en_vadj, en_rx, sw_pa_onoff, sw_rx_tddfdd, sw_rxtx) MAKE_M2_DSDR_P_REG_WR(CHA, \
    (((en_tx) << CHA_EN_TX_OFF) & CHA_EN_TX_MSK) |  \
    (((en_vadj) << CHA_EN_VADJ_OFF) & CHA_EN_VADJ_MSK) |  \
    (((en_rx) << CHA_EN_RX_OFF) & CHA_EN_RX_MSK) |  \
    (((sw_pa_onoff) << CHA_SW_PA_ONOFF_OFF) & CHA_SW_PA_ONOFF_MSK) |  \
    ((((((sw_rx_tddfdd) >> 1) & 0x1) << 3) | ((((sw_rx_tddfdd) >> 0) & 0x1) << 1)) & CHA_SW_RX_TDDFDD_MSK) |  \
    (((sw_rxtx) << CHA_SW_RXTX_OFF) & CHA_SW_RXTX_MSK))
// Register R18 [0x12] -- CHB
enum chb_sw_rx_tddfdd_options {
    CHB_SW_RX_TDDFDD_REV0_LNA_TO_RX_____REV2_SHUTDOWN = 0,
    CHB_SW_RX_TDDFDD_REV0_LNA_TO_TDDSW__REV2_LNA_TO_TDDSW = 1,
    CHB_SW_RX_TDDFDD_REV0_LNA_TO_RX_____REV2_LNA_TO_LB = 2,
    CHB_SW_RX_TDDFDD_REV0_LNA_TO_TDDSW__REV2_LNA_TO_RX = 3,
};

enum chb_fields_t {
    CHB_EN_TX_OFF = 0x7,
    CHB_EN_TX_MSK = 0x80,
    CHB_EN_VADJ_OFF = 0x6,
    CHB_EN_VADJ_MSK = 0x40,
    CHB_EN_RX_OFF = 0x5,
    CHB_EN_RX_MSK = 0x20,
    CHB_SW_PA_ONOFF_OFF = 0x2,
    CHB_SW_PA_ONOFF_MSK = 0x4,
    CHB_SW_RX_TDDFDD_OFF = 0x1,
    CHB_SW_RX_TDDFDD_MSK = 0xa,
    CHB_SW_RXTX_OFF = 0x0,
    CHB_SW_RXTX_MSK = 0x1,
};
#define GET_M2_DSDR_P_CHB_EN_TX(x) (((x) & CHB_EN_TX_MSK) >> CHB_EN_TX_OFF)
#define GET_M2_DSDR_P_CHB_EN_VADJ(x) (((x) & CHB_EN_VADJ_MSK) >> CHB_EN_VADJ_OFF)
#define GET_M2_DSDR_P_CHB_EN_RX(x) (((x) & CHB_EN_RX_MSK) >> CHB_EN_RX_OFF)
#define GET_M2_DSDR_P_CHB_SW_PA_ONOFF(x) (((x) & CHB_SW_PA_ONOFF_MSK) >> CHB_SW_PA_ONOFF_OFF)
#define GET_M2_DSDR_P_CHB_SW_RX_TDDFDD(x) ((((((x) & CHB_SW_RX_TDDFDD_MSK) >> 1) & 0x1) << 0) | (((((x) & CHB_SW_RX_TDDFDD_MSK) >> 3) & 0x1) << 1))
#define GET_M2_DSDR_P_CHB_SW_RXTX(x) (((x) & CHB_SW_RXTX_MSK) >> CHB_SW_RXTX_OFF)
#define SET_M2_DSDR_P_CHB_EN_TX(p, f) (p) = ((p) & ~CHB_EN_TX_MSK) | (((f) << CHB_EN_TX_OFF) & CHB_EN_TX_MSK)
#define SET_M2_DSDR_P_CHB_EN_VADJ(p, f) (p) = ((p) & ~CHB_EN_VADJ_MSK) | (((f) << CHB_EN_VADJ_OFF) & CHB_EN_VADJ_MSK)
#define SET_M2_DSDR_P_CHB_EN_RX(p, f) (p) = ((p) & ~CHB_EN_RX_MSK) | (((f) << CHB_EN_RX_OFF) & CHB_EN_RX_MSK)
#define SET_M2_DSDR_P_CHB_SW_PA_ONOFF(p, f) (p) = ((p) & ~CHB_SW_PA_ONOFF_MSK) | (((f) << CHB_SW_PA_ONOFF_OFF) & CHB_SW_PA_ONOFF_MSK)
#define SET_M2_DSDR_P_CHB_SW_RX_TDDFDD(p, f) (p) = ((p) & ~CHB_SW_RX_TDDFDD_MSK) | ((((((f) >> 1) & 0x1) << 3) | ((((f) >> 0) & 0x1) << 1)) & CHB_SW_RX_TDDFDD_MSK)
#define SET_M2_DSDR_P_CHB_SW_RXTX(p, f) (p) = ((p) & ~CHB_SW_RXTX_MSK) | (((f) << CHB_SW_RXTX_OFF) & CHB_SW_RXTX_MSK)

#define MAKE_M2_DSDR_P_CHB(en_tx, en_vadj, en_rx, sw_pa_onoff, sw_rx_tddfdd, sw_rxtx) MAKE_M2_DSDR_P_REG_WR(CHB, \
    (((en_tx) << CHB_EN_TX_OFF) & CHB_EN_TX_MSK) |  \
    (((en_vadj) << CHB_EN_VADJ_OFF) & CHB_EN_VADJ_MSK) |  \
    (((en_rx) << CHB_EN_RX_OFF) & CHB_EN_RX_MSK) |  \
    (((sw_pa_onoff) << CHB_SW_PA_ONOFF_OFF) & CHB_SW_PA_ONOFF_MSK) |  \
    ((((((sw_rx_tddfdd) >> 1) & 0x1) << 3) | ((((sw_rx_tddfdd) >> 0) & 0x1) << 1)) & CHB_SW_RX_TDDFDD_MSK) |  \
    (((sw_rxtx) << CHB_SW_RXTX_OFF) & CHB_SW_RXTX_MSK))
// Register R19 [0x13] -- CHC
enum chc_sw_rx_tddfdd_options {
    CHC_SW_RX_TDDFDD_REV0_LNA_TO_RX_____REV2_SHUTDOWN = 0,
    CHC_SW_RX_TDDFDD_REV0_LNA_TO_TDDSW__REV2_LNA_TO_TDDSW = 1,
    CHC_SW_RX_TDDFDD_REV0_LNA_TO_RX_____REV2_LNA_TO_LB = 2,
    CHC_SW_RX_TDDFDD_REV0_LNA_TO_TDDSW__REV2_LNA_TO_RX = 3,
};

enum chc_fields_t {
    CHC_EN_TX_OFF = 0x7,
    CHC_EN_TX_MSK = 0x80,
    CHC_EN_VADJ_OFF = 0x6,
    CHC_EN_VADJ_MSK = 0x40,
    CHC_EN_RX_OFF = 0x5,
    CHC_EN_RX_MSK = 0x20,
    CHC_SW_PA_ONOFF_OFF = 0x2,
    CHC_SW_PA_ONOFF_MSK = 0x4,
    CHC_SW_RX_TDDFDD_OFF = 0x1,
    CHC_SW_RX_TDDFDD_MSK = 0xa,
    CHC_SW_RXTX_OFF = 0x0,
    CHC_SW_RXTX_MSK = 0x1,
};
#define GET_M2_DSDR_P_CHC_EN_TX(x) (((x) & CHC_EN_TX_MSK) >> CHC_EN_TX_OFF)
#define GET_M2_DSDR_P_CHC_EN_VADJ(x) (((x) & CHC_EN_VADJ_MSK) >> CHC_EN_VADJ_OFF)
#define GET_M2_DSDR_P_CHC_EN_RX(x) (((x) & CHC_EN_RX_MSK) >> CHC_EN_RX_OFF)
#define GET_M2_DSDR_P_CHC_SW_PA_ONOFF(x) (((x) & CHC_SW_PA_ONOFF_MSK) >> CHC_SW_PA_ONOFF_OFF)
#define GET_M2_DSDR_P_CHC_SW_RX_TDDFDD(x) ((((((x) & CHC_SW_RX_TDDFDD_MSK) >> 1) & 0x1) << 0) | (((((x) & CHC_SW_RX_TDDFDD_MSK) >> 3) & 0x1) << 1))
#define GET_M2_DSDR_P_CHC_SW_RXTX(x) (((x) & CHC_SW_RXTX_MSK) >> CHC_SW_RXTX_OFF)
#define SET_M2_DSDR_P_CHC_EN_TX(p, f) (p) = ((p) & ~CHC_EN_TX_MSK) | (((f) << CHC_EN_TX_OFF) & CHC_EN_TX_MSK)
#define SET_M2_DSDR_P_CHC_EN_VADJ(p, f) (p) = ((p) & ~CHC_EN_VADJ_MSK) | (((f) << CHC_EN_VADJ_OFF) & CHC_EN_VADJ_MSK)
#define SET_M2_DSDR_P_CHC_EN_RX(p, f) (p) = ((p) & ~CHC_EN_RX_MSK) | (((f) << CHC_EN_RX_OFF) & CHC_EN_RX_MSK)
#define SET_M2_DSDR_P_CHC_SW_PA_ONOFF(p, f) (p) = ((p) & ~CHC_SW_PA_ONOFF_MSK) | (((f) << CHC_SW_PA_ONOFF_OFF) & CHC_SW_PA_ONOFF_MSK)
#define SET_M2_DSDR_P_CHC_SW_RX_TDDFDD(p, f) (p) = ((p) & ~CHC_SW_RX_TDDFDD_MSK) | ((((((f) >> 1) & 0x1) << 3) | ((((f) >> 0) & 0x1) << 1)) & CHC_SW_RX_TDDFDD_MSK)
#define SET_M2_DSDR_P_CHC_SW_RXTX(p, f) (p) = ((p) & ~CHC_SW_RXTX_MSK) | (((f) << CHC_SW_RXTX_OFF) & CHC_SW_RXTX_MSK)

#define MAKE_M2_DSDR_P_CHC(en_tx, en_vadj, en_rx, sw_pa_onoff, sw_rx_tddfdd, sw_rxtx) MAKE_M2_DSDR_P_REG_WR(CHC, \
    (((en_tx) << CHC_EN_TX_OFF) & CHC_EN_TX_MSK) |  \
    (((en_vadj) << CHC_EN_VADJ_OFF) & CHC_EN_VADJ_MSK) |  \
    (((en_rx) << CHC_EN_RX_OFF) & CHC_EN_RX_MSK) |  \
    (((sw_pa_onoff) << CHC_SW_PA_ONOFF_OFF) & CHC_SW_PA_ONOFF_MSK) |  \
    ((((((sw_rx_tddfdd) >> 1) & 0x1) << 3) | ((((sw_rx_tddfdd) >> 0) & 0x1) << 1)) & CHC_SW_RX_TDDFDD_MSK) |  \
    (((sw_rxtx) << CHC_SW_RXTX_OFF) & CHC_SW_RXTX_MSK))
// Register R20 [0x14] -- CHD
enum chd_sw_rx_tddfdd_options {
    CHD_SW_RX_TDDFDD_REV0_LNA_TO_RX_____REV2_SHUTDOWN = 0,
    CHD_SW_RX_TDDFDD_REV0_LNA_TO_TDDSW__REV2_LNA_TO_TDDSW = 1,
    CHD_SW_RX_TDDFDD_REV0_LNA_TO_RX_____REV2_LNA_TO_LB = 2,
    CHD_SW_RX_TDDFDD_REV0_LNA_TO_TDDSW__REV2_LNA_TO_RX = 3,
};

enum chd_fields_t {
    CHD_EN_TX_OFF = 0x7,
    CHD_EN_TX_MSK = 0x80,
    CHD_EN_VADJ_OFF = 0x6,
    CHD_EN_VADJ_MSK = 0x40,
    CHD_EN_RX_OFF = 0x5,
    CHD_EN_RX_MSK = 0x20,
    CHD_SW_PA_ONOFF_OFF = 0x2,
    CHD_SW_PA_ONOFF_MSK = 0x4,
    CHD_SW_RX_TDDFDD_OFF = 0x1,
    CHD_SW_RX_TDDFDD_MSK = 0xa,
    CHD_SW_RXTX_OFF = 0x0,
    CHD_SW_RXTX_MSK = 0x1,
};
#define GET_M2_DSDR_P_CHD_EN_TX(x) (((x) & CHD_EN_TX_MSK) >> CHD_EN_TX_OFF)
#define GET_M2_DSDR_P_CHD_EN_VADJ(x) (((x) & CHD_EN_VADJ_MSK) >> CHD_EN_VADJ_OFF)
#define GET_M2_DSDR_P_CHD_EN_RX(x) (((x) & CHD_EN_RX_MSK) >> CHD_EN_RX_OFF)
#define GET_M2_DSDR_P_CHD_SW_PA_ONOFF(x) (((x) & CHD_SW_PA_ONOFF_MSK) >> CHD_SW_PA_ONOFF_OFF)
#define GET_M2_DSDR_P_CHD_SW_RX_TDDFDD(x) ((((((x) & CHD_SW_RX_TDDFDD_MSK) >> 1) & 0x1) << 0) | (((((x) & CHD_SW_RX_TDDFDD_MSK) >> 3) & 0x1) << 1))
#define GET_M2_DSDR_P_CHD_SW_RXTX(x) (((x) & CHD_SW_RXTX_MSK) >> CHD_SW_RXTX_OFF)
#define SET_M2_DSDR_P_CHD_EN_TX(p, f) (p) = ((p) & ~CHD_EN_TX_MSK) | (((f) << CHD_EN_TX_OFF) & CHD_EN_TX_MSK)
#define SET_M2_DSDR_P_CHD_EN_VADJ(p, f) (p) = ((p) & ~CHD_EN_VADJ_MSK) | (((f) << CHD_EN_VADJ_OFF) & CHD_EN_VADJ_MSK)
#define SET_M2_DSDR_P_CHD_EN_RX(p, f) (p) = ((p) & ~CHD_EN_RX_MSK) | (((f) << CHD_EN_RX_OFF) & CHD_EN_RX_MSK)
#define SET_M2_DSDR_P_CHD_SW_PA_ONOFF(p, f) (p) = ((p) & ~CHD_SW_PA_ONOFF_MSK) | (((f) << CHD_SW_PA_ONOFF_OFF) & CHD_SW_PA_ONOFF_MSK)
#define SET_M2_DSDR_P_CHD_SW_RX_TDDFDD(p, f) (p) = ((p) & ~CHD_SW_RX_TDDFDD_MSK) | ((((((f) >> 1) & 0x1) << 3) | ((((f) >> 0) & 0x1) << 1)) & CHD_SW_RX_TDDFDD_MSK)
#define SET_M2_DSDR_P_CHD_SW_RXTX(p, f) (p) = ((p) & ~CHD_SW_RXTX_MSK) | (((f) << CHD_SW_RXTX_OFF) & CHD_SW_RXTX_MSK)

#define MAKE_M2_DSDR_P_CHD(en_tx, en_vadj, en_rx, sw_pa_onoff, sw_rx_tddfdd, sw_rxtx) MAKE_M2_DSDR_P_REG_WR(CHD, \
    (((en_tx) << CHD_EN_TX_OFF) & CHD_EN_TX_MSK) |  \
    (((en_vadj) << CHD_EN_VADJ_OFF) & CHD_EN_VADJ_MSK) |  \
    (((en_rx) << CHD_EN_RX_OFF) & CHD_EN_RX_MSK) |  \
    (((sw_pa_onoff) << CHD_SW_PA_ONOFF_OFF) & CHD_SW_PA_ONOFF_MSK) |  \
    ((((((sw_rx_tddfdd) >> 1) & 0x1) << 3) | ((((sw_rx_tddfdd) >> 0) & 0x1) << 1)) & CHD_SW_RX_TDDFDD_MSK) |  \
    (((sw_rxtx) << CHD_SW_RXTX_OFF) & CHD_SW_RXTX_MSK))
// Register R21 [0x15] -- ATT_RX_CHA

enum att_rx_cha_fields_t {
    ATT_RX_CHA_VAL_OFF = 0x0,
    ATT_RX_CHA_VAL_MSK = 0xf,
};
#define GET_M2_DSDR_P_ATT_RX_CHA_VAL(x) (((x) & ATT_RX_CHA_VAL_MSK) >> ATT_RX_CHA_VAL_OFF)
#define SET_M2_DSDR_P_ATT_RX_CHA_VAL(p, f) (p) = ((p) & ~ATT_RX_CHA_VAL_MSK) | (((f) << ATT_RX_CHA_VAL_OFF) & ATT_RX_CHA_VAL_MSK)

#define MAKE_M2_DSDR_P_ATT_RX_CHA(val) MAKE_M2_DSDR_P_REG_WR(ATT_RX_CHA, \
    (((val) << ATT_RX_CHA_VAL_OFF) & ATT_RX_CHA_VAL_MSK))
// Register R22 [0x16] -- ATT_RX_CHB

enum att_rx_chb_fields_t {
    ATT_RX_CHB_VAL_OFF = 0x0,
    ATT_RX_CHB_VAL_MSK = 0xf,
};
#define GET_M2_DSDR_P_ATT_RX_CHB_VAL(x) (((x) & ATT_RX_CHB_VAL_MSK) >> ATT_RX_CHB_VAL_OFF)
#define SET_M2_DSDR_P_ATT_RX_CHB_VAL(p, f) (p) = ((p) & ~ATT_RX_CHB_VAL_MSK) | (((f) << ATT_RX_CHB_VAL_OFF) & ATT_RX_CHB_VAL_MSK)

#define MAKE_M2_DSDR_P_ATT_RX_CHB(val) MAKE_M2_DSDR_P_REG_WR(ATT_RX_CHB, \
    (((val) << ATT_RX_CHB_VAL_OFF) & ATT_RX_CHB_VAL_MSK))
// Register R23 [0x17] -- ATT_RX_CHC

enum att_rx_chc_fields_t {
    ATT_RX_CHC_VAL_OFF = 0x0,
    ATT_RX_CHC_VAL_MSK = 0xf,
};
#define GET_M2_DSDR_P_ATT_RX_CHC_VAL(x) (((x) & ATT_RX_CHC_VAL_MSK) >> ATT_RX_CHC_VAL_OFF)
#define SET_M2_DSDR_P_ATT_RX_CHC_VAL(p, f) (p) = ((p) & ~ATT_RX_CHC_VAL_MSK) | (((f) << ATT_RX_CHC_VAL_OFF) & ATT_RX_CHC_VAL_MSK)

#define MAKE_M2_DSDR_P_ATT_RX_CHC(val) MAKE_M2_DSDR_P_REG_WR(ATT_RX_CHC, \
    (((val) << ATT_RX_CHC_VAL_OFF) & ATT_RX_CHC_VAL_MSK))
// Register R24 [0x18] -- ATT_RX_CHD

enum att_rx_chd_fields_t {
    ATT_RX_CHD_VAL_OFF = 0x0,
    ATT_RX_CHD_VAL_MSK = 0xf,
};
#define GET_M2_DSDR_P_ATT_RX_CHD_VAL(x) (((x) & ATT_RX_CHD_VAL_MSK) >> ATT_RX_CHD_VAL_OFF)
#define SET_M2_DSDR_P_ATT_RX_CHD_VAL(p, f) (p) = ((p) & ~ATT_RX_CHD_VAL_MSK) | (((f) << ATT_RX_CHD_VAL_OFF) & ATT_RX_CHD_VAL_MSK)

#define MAKE_M2_DSDR_P_ATT_RX_CHD(val) MAKE_M2_DSDR_P_REG_WR(ATT_RX_CHD, \
    (((val) << ATT_RX_CHD_VAL_OFF) & ATT_RX_CHD_VAL_MSK))
