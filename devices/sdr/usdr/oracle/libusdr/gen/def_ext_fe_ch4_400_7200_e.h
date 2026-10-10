enum ext_fe_ch4_400_7200_e_regs_t {
    SW_RX_FILTER = 0x30,
    ENABLE = 0x31,
    LED_TRX_CTRL = 0x32,
    LEDRX_CH_CTRL = 0x33,
    P_A_EN_AB = 0x34,
    ATTN_RX_CH_AB = 0x35,
    SW_AB = 0x36,
    P_A_EN_CD = 0x37,
    ATTN_RX_CH_CD = 0x38,
    SW_CD = 0x39,
};
#define MAKE_EXT_FE_CH4_400_7200_E_REG_WR(a, v) (0x80000000 | ((a) << 24) | ((v) & 0xffffff))
#define MAKE_EXT_FE_CH4_400_7200_E_REG_RD(a) (((a) << 24))
enum ext_fe_ch4_400_7200_e_rx_filt_in_opts_t {
    RX_FILT_IN_OPTS_MUTE0 = 0x0,
    RX_FILT_IN_OPTS_50_1000M = 0x3,
    RX_FILT_IN_OPTS_1000_2000M = 0x2,
    RX_FILT_IN_OPTS_2000_3500M = 0x1,
    RX_FILT_IN_OPTS_2500_5000M = 0x5,
    RX_FILT_IN_OPTS_3500_7100M = 0x4,
    RX_FILT_IN_OPTS_MUTE1 = 0x6,
    RX_FILT_IN_OPTS_MUTE2 = 0x7,
};
enum ext_fe_ch4_400_7200_e_rx_filt_out_opts_t {
    RX_FILT_OUT_OPTS_MUTE0 = 0x0,
    RX_FILT_OUT_OPTS_50_1000M = 0x3,
    RX_FILT_OUT_OPTS_1000_2000M = 0x4,
    RX_FILT_OUT_OPTS_2000_3500M = 0x5,
    RX_FILT_OUT_OPTS_2500_5000M = 0x1,
    RX_FILT_OUT_OPTS_3500_7100M = 0x2,
    RX_FILT_OUT_OPTS_MUTE1 = 0x6,
    RX_FILT_OUT_OPTS_MUTE2 = 0x7,
};
// Register R48 [0x30] -- SW_RX_FILTER
enum sw_rx_filter_in_chd_options {
    SW_RX_FILTER_IN_CHD_MUTE0 = 0,
    SW_RX_FILTER_IN_CHD_2000_3500M = 1,
    SW_RX_FILTER_IN_CHD_1000_2000M = 2,
    SW_RX_FILTER_IN_CHD_50_1000M = 3,
    SW_RX_FILTER_IN_CHD_3500_7100M = 4,
    SW_RX_FILTER_IN_CHD_2500_5000M = 5,
    SW_RX_FILTER_IN_CHD_MUTE1 = 6,
    SW_RX_FILTER_IN_CHD_MUTE2 = 7,
};
enum sw_rx_filter_in_chc_options {
    SW_RX_FILTER_IN_CHC_MUTE0 = 0,
    SW_RX_FILTER_IN_CHC_2000_3500M = 1,
    SW_RX_FILTER_IN_CHC_1000_2000M = 2,
    SW_RX_FILTER_IN_CHC_50_1000M = 3,
    SW_RX_FILTER_IN_CHC_3500_7100M = 4,
    SW_RX_FILTER_IN_CHC_2500_5000M = 5,
    SW_RX_FILTER_IN_CHC_MUTE1 = 6,
    SW_RX_FILTER_IN_CHC_MUTE2 = 7,
};
enum sw_rx_filter_in_chb_options {
    SW_RX_FILTER_IN_CHB_MUTE0 = 0,
    SW_RX_FILTER_IN_CHB_2000_3500M = 1,
    SW_RX_FILTER_IN_CHB_1000_2000M = 2,
    SW_RX_FILTER_IN_CHB_50_1000M = 3,
    SW_RX_FILTER_IN_CHB_3500_7100M = 4,
    SW_RX_FILTER_IN_CHB_2500_5000M = 5,
    SW_RX_FILTER_IN_CHB_MUTE1 = 6,
    SW_RX_FILTER_IN_CHB_MUTE2 = 7,
};
enum sw_rx_filter_in_cha_options {
    SW_RX_FILTER_IN_CHA_MUTE0 = 0,
    SW_RX_FILTER_IN_CHA_2000_3500M = 1,
    SW_RX_FILTER_IN_CHA_1000_2000M = 2,
    SW_RX_FILTER_IN_CHA_50_1000M = 3,
    SW_RX_FILTER_IN_CHA_3500_7100M = 4,
    SW_RX_FILTER_IN_CHA_2500_5000M = 5,
    SW_RX_FILTER_IN_CHA_MUTE1 = 6,
    SW_RX_FILTER_IN_CHA_MUTE2 = 7,
};
enum sw_rx_filter_out_cha_options {
    SW_RX_FILTER_OUT_CHA_MUTE0 = 0,
    SW_RX_FILTER_OUT_CHA_2500_5000M = 1,
    SW_RX_FILTER_OUT_CHA_3500_7100M = 2,
    SW_RX_FILTER_OUT_CHA_50_1000M = 3,
    SW_RX_FILTER_OUT_CHA_1000_2000M = 4,
    SW_RX_FILTER_OUT_CHA_2000_3500M = 5,
    SW_RX_FILTER_OUT_CHA_MUTE1 = 6,
    SW_RX_FILTER_OUT_CHA_MUTE2 = 7,
};
enum sw_rx_filter_out_chb_options {
    SW_RX_FILTER_OUT_CHB_MUTE0 = 0,
    SW_RX_FILTER_OUT_CHB_2500_5000M = 1,
    SW_RX_FILTER_OUT_CHB_3500_7100M = 2,
    SW_RX_FILTER_OUT_CHB_50_1000M = 3,
    SW_RX_FILTER_OUT_CHB_1000_2000M = 4,
    SW_RX_FILTER_OUT_CHB_2000_3500M = 5,
    SW_RX_FILTER_OUT_CHB_MUTE1 = 6,
    SW_RX_FILTER_OUT_CHB_MUTE2 = 7,
};
enum sw_rx_filter_out_chc_options {
    SW_RX_FILTER_OUT_CHC_MUTE0 = 0,
    SW_RX_FILTER_OUT_CHC_2500_5000M = 1,
    SW_RX_FILTER_OUT_CHC_3500_7100M = 2,
    SW_RX_FILTER_OUT_CHC_50_1000M = 3,
    SW_RX_FILTER_OUT_CHC_1000_2000M = 4,
    SW_RX_FILTER_OUT_CHC_2000_3500M = 5,
    SW_RX_FILTER_OUT_CHC_MUTE1 = 6,
    SW_RX_FILTER_OUT_CHC_MUTE2 = 7,
};
enum sw_rx_filter_out_chd_options {
    SW_RX_FILTER_OUT_CHD_MUTE0 = 0,
    SW_RX_FILTER_OUT_CHD_2500_5000M = 1,
    SW_RX_FILTER_OUT_CHD_3500_7100M = 2,
    SW_RX_FILTER_OUT_CHD_50_1000M = 3,
    SW_RX_FILTER_OUT_CHD_1000_2000M = 4,
    SW_RX_FILTER_OUT_CHD_2000_3500M = 5,
    SW_RX_FILTER_OUT_CHD_MUTE1 = 6,
    SW_RX_FILTER_OUT_CHD_MUTE2 = 7,
};

enum sw_rx_filter_fields_t {
    SW_RX_FILTER_IN_CHD_OFF = 0xb,
    SW_RX_FILTER_IN_CHD_MSK = 0x30800,
    SW_RX_FILTER_IN_CHC_OFF = 0xa,
    SW_RX_FILTER_IN_CHC_MSK = 0xc0400,
    SW_RX_FILTER_IN_CHB_OFF = 0x9,
    SW_RX_FILTER_IN_CHB_MSK = 0x300200,
    SW_RX_FILTER_IN_CHA_OFF = 0x8,
    SW_RX_FILTER_IN_CHA_MSK = 0xc00100,
    SW_RX_FILTER_OUT_CHA_OFF = 0x6,
    SW_RX_FILTER_OUT_CHA_MSK = 0x80c0,
    SW_RX_FILTER_OUT_CHB_OFF = 0x4,
    SW_RX_FILTER_OUT_CHB_MSK = 0x4030,
    SW_RX_FILTER_OUT_CHC_OFF = 0x2,
    SW_RX_FILTER_OUT_CHC_MSK = 0x200c,
    SW_RX_FILTER_OUT_CHD_OFF = 0x0,
    SW_RX_FILTER_OUT_CHD_MSK = 0x1003,
};
#define GET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_IN_CHD(x) ((((((x) & SW_RX_FILTER_IN_CHD_MSK) >> 16) & 0x1) << 0) | (((((x) & SW_RX_FILTER_IN_CHD_MSK) >> 17) & 0x1) << 1) | (((((x) & SW_RX_FILTER_IN_CHD_MSK) >> 11) & 0x1) << 2))
#define GET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_IN_CHC(x) ((((((x) & SW_RX_FILTER_IN_CHC_MSK) >> 18) & 0x1) << 0) | (((((x) & SW_RX_FILTER_IN_CHC_MSK) >> 19) & 0x1) << 1) | (((((x) & SW_RX_FILTER_IN_CHC_MSK) >> 10) & 0x1) << 2))
#define GET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_IN_CHB(x) ((((((x) & SW_RX_FILTER_IN_CHB_MSK) >> 20) & 0x1) << 0) | (((((x) & SW_RX_FILTER_IN_CHB_MSK) >> 21) & 0x1) << 1) | (((((x) & SW_RX_FILTER_IN_CHB_MSK) >> 9) & 0x1) << 2))
#define GET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_IN_CHA(x) ((((((x) & SW_RX_FILTER_IN_CHA_MSK) >> 22) & 0x1) << 0) | (((((x) & SW_RX_FILTER_IN_CHA_MSK) >> 23) & 0x1) << 1) | (((((x) & SW_RX_FILTER_IN_CHA_MSK) >> 8) & 0x1) << 2))
#define GET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_OUT_CHA(x) ((((((x) & SW_RX_FILTER_OUT_CHA_MSK) >> 7) & 0x1) << 0) | (((((x) & SW_RX_FILTER_OUT_CHA_MSK) >> 6) & 0x1) << 1) | (((((x) & SW_RX_FILTER_OUT_CHA_MSK) >> 15) & 0x1) << 2))
#define GET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_OUT_CHB(x) ((((((x) & SW_RX_FILTER_OUT_CHB_MSK) >> 5) & 0x1) << 0) | (((((x) & SW_RX_FILTER_OUT_CHB_MSK) >> 4) & 0x1) << 1) | (((((x) & SW_RX_FILTER_OUT_CHB_MSK) >> 14) & 0x1) << 2))
#define GET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_OUT_CHC(x) ((((((x) & SW_RX_FILTER_OUT_CHC_MSK) >> 2) & 0x1) << 0) | (((((x) & SW_RX_FILTER_OUT_CHC_MSK) >> 3) & 0x1) << 1) | (((((x) & SW_RX_FILTER_OUT_CHC_MSK) >> 13) & 0x1) << 2))
#define GET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_OUT_CHD(x) ((((((x) & SW_RX_FILTER_OUT_CHD_MSK) >> 1) & 0x1) << 0) | (((((x) & SW_RX_FILTER_OUT_CHD_MSK) >> 0) & 0x1) << 1) | (((((x) & SW_RX_FILTER_OUT_CHD_MSK) >> 12) & 0x1) << 2))
#define SET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_IN_CHD(p, f) (p) = ((p) & ~SW_RX_FILTER_IN_CHD_MSK) | ((((((f) >> 2) & 0x1) << 11) | ((((f) >> 1) & 0x1) << 17) | ((((f) >> 0) & 0x1) << 16)) & SW_RX_FILTER_IN_CHD_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_IN_CHC(p, f) (p) = ((p) & ~SW_RX_FILTER_IN_CHC_MSK) | ((((((f) >> 2) & 0x1) << 10) | ((((f) >> 1) & 0x1) << 19) | ((((f) >> 0) & 0x1) << 18)) & SW_RX_FILTER_IN_CHC_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_IN_CHB(p, f) (p) = ((p) & ~SW_RX_FILTER_IN_CHB_MSK) | ((((((f) >> 2) & 0x1) << 9) | ((((f) >> 1) & 0x1) << 21) | ((((f) >> 0) & 0x1) << 20)) & SW_RX_FILTER_IN_CHB_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_IN_CHA(p, f) (p) = ((p) & ~SW_RX_FILTER_IN_CHA_MSK) | ((((((f) >> 2) & 0x1) << 8) | ((((f) >> 1) & 0x1) << 23) | ((((f) >> 0) & 0x1) << 22)) & SW_RX_FILTER_IN_CHA_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_OUT_CHA(p, f) (p) = ((p) & ~SW_RX_FILTER_OUT_CHA_MSK) | ((((((f) >> 2) & 0x1) << 15) | ((((f) >> 1) & 0x1) << 6) | ((((f) >> 0) & 0x1) << 7)) & SW_RX_FILTER_OUT_CHA_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_OUT_CHB(p, f) (p) = ((p) & ~SW_RX_FILTER_OUT_CHB_MSK) | ((((((f) >> 2) & 0x1) << 14) | ((((f) >> 1) & 0x1) << 4) | ((((f) >> 0) & 0x1) << 5)) & SW_RX_FILTER_OUT_CHB_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_OUT_CHC(p, f) (p) = ((p) & ~SW_RX_FILTER_OUT_CHC_MSK) | ((((((f) >> 2) & 0x1) << 13) | ((((f) >> 1) & 0x1) << 3) | ((((f) >> 0) & 0x1) << 2)) & SW_RX_FILTER_OUT_CHC_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_RX_FILTER_OUT_CHD(p, f) (p) = ((p) & ~SW_RX_FILTER_OUT_CHD_MSK) | ((((((f) >> 2) & 0x1) << 12) | ((((f) >> 1) & 0x1) << 0) | ((((f) >> 0) & 0x1) << 1)) & SW_RX_FILTER_OUT_CHD_MSK)

#define MAKE_EXT_FE_CH4_400_7200_E_SW_RX_FILTER(in_chd, in_chc, in_chb, in_cha, out_cha, out_chb, out_chc, out_chd) MAKE_EXT_FE_CH4_400_7200_E_REG_WR(SW_RX_FILTER, \
    ((((((in_chd) >> 2) & 0x1) << 11) | ((((in_chd) >> 1) & 0x1) << 17) | ((((in_chd) >> 0) & 0x1) << 16)) & SW_RX_FILTER_IN_CHD_MSK) |  \
    ((((((in_chc) >> 2) & 0x1) << 10) | ((((in_chc) >> 1) & 0x1) << 19) | ((((in_chc) >> 0) & 0x1) << 18)) & SW_RX_FILTER_IN_CHC_MSK) |  \
    ((((((in_chb) >> 2) & 0x1) << 9) | ((((in_chb) >> 1) & 0x1) << 21) | ((((in_chb) >> 0) & 0x1) << 20)) & SW_RX_FILTER_IN_CHB_MSK) |  \
    ((((((in_cha) >> 2) & 0x1) << 8) | ((((in_cha) >> 1) & 0x1) << 23) | ((((in_cha) >> 0) & 0x1) << 22)) & SW_RX_FILTER_IN_CHA_MSK) |  \
    ((((((out_cha) >> 2) & 0x1) << 15) | ((((out_cha) >> 1) & 0x1) << 6) | ((((out_cha) >> 0) & 0x1) << 7)) & SW_RX_FILTER_OUT_CHA_MSK) |  \
    ((((((out_chb) >> 2) & 0x1) << 14) | ((((out_chb) >> 1) & 0x1) << 4) | ((((out_chb) >> 0) & 0x1) << 5)) & SW_RX_FILTER_OUT_CHB_MSK) |  \
    ((((((out_chc) >> 2) & 0x1) << 13) | ((((out_chc) >> 1) & 0x1) << 3) | ((((out_chc) >> 0) & 0x1) << 2)) & SW_RX_FILTER_OUT_CHC_MSK) |  \
    ((((((out_chd) >> 2) & 0x1) << 12) | ((((out_chd) >> 1) & 0x1) << 0) | ((((out_chd) >> 0) & 0x1) << 1)) & SW_RX_FILTER_OUT_CHD_MSK))
// Register R49 [0x31] -- ENABLE

enum enable_fields_t {
    ENABLE_PA_BYPASS_CHD_OFF = 0x7,
    ENABLE_PA_BYPASS_CHD_MSK = 0x80,
    ENABLE_PA_BYPASS_CHC_OFF = 0x6,
    ENABLE_PA_BYPASS_CHC_MSK = 0x40,
    ENABLE_PA_BYPASS_CHB_OFF = 0x5,
    ENABLE_PA_BYPASS_CHB_MSK = 0x20,
    ENABLE_PA_BYPASS_CHA_OFF = 0x4,
    ENABLE_PA_BYPASS_CHA_MSK = 0x10,
    ENABLE_IF_VBYP_OFF = 0x3,
    ENABLE_IF_VBYP_MSK = 0x8,
    ENABLE_REF_GPS_OFF = 0x2,
    ENABLE_REF_GPS_MSK = 0x4,
    ENABLE_P8V_TX_OFF = 0x1,
    ENABLE_P8V_TX_MSK = 0x2,
    ENABLE_P6V_RX_OFF = 0x0,
    ENABLE_P6V_RX_MSK = 0x1,
};
#define GET_EXT_FE_CH4_400_7200_E_ENABLE_PA_BYPASS_CHD(x) (((x) & ENABLE_PA_BYPASS_CHD_MSK) >> ENABLE_PA_BYPASS_CHD_OFF)
#define GET_EXT_FE_CH4_400_7200_E_ENABLE_PA_BYPASS_CHC(x) (((x) & ENABLE_PA_BYPASS_CHC_MSK) >> ENABLE_PA_BYPASS_CHC_OFF)
#define GET_EXT_FE_CH4_400_7200_E_ENABLE_PA_BYPASS_CHB(x) (((x) & ENABLE_PA_BYPASS_CHB_MSK) >> ENABLE_PA_BYPASS_CHB_OFF)
#define GET_EXT_FE_CH4_400_7200_E_ENABLE_PA_BYPASS_CHA(x) (((x) & ENABLE_PA_BYPASS_CHA_MSK) >> ENABLE_PA_BYPASS_CHA_OFF)
#define GET_EXT_FE_CH4_400_7200_E_ENABLE_IF_VBYP(x) (((x) & ENABLE_IF_VBYP_MSK) >> ENABLE_IF_VBYP_OFF)
#define GET_EXT_FE_CH4_400_7200_E_ENABLE_REF_GPS(x) (((x) & ENABLE_REF_GPS_MSK) >> ENABLE_REF_GPS_OFF)
#define GET_EXT_FE_CH4_400_7200_E_ENABLE_P8V_TX(x) (((x) & ENABLE_P8V_TX_MSK) >> ENABLE_P8V_TX_OFF)
#define GET_EXT_FE_CH4_400_7200_E_ENABLE_P6V_RX(x) (((x) & ENABLE_P6V_RX_MSK) >> ENABLE_P6V_RX_OFF)
#define SET_EXT_FE_CH4_400_7200_E_ENABLE_PA_BYPASS_CHD(p, f) (p) = ((p) & ~ENABLE_PA_BYPASS_CHD_MSK) | (((f) << ENABLE_PA_BYPASS_CHD_OFF) & ENABLE_PA_BYPASS_CHD_MSK)
#define SET_EXT_FE_CH4_400_7200_E_ENABLE_PA_BYPASS_CHC(p, f) (p) = ((p) & ~ENABLE_PA_BYPASS_CHC_MSK) | (((f) << ENABLE_PA_BYPASS_CHC_OFF) & ENABLE_PA_BYPASS_CHC_MSK)
#define SET_EXT_FE_CH4_400_7200_E_ENABLE_PA_BYPASS_CHB(p, f) (p) = ((p) & ~ENABLE_PA_BYPASS_CHB_MSK) | (((f) << ENABLE_PA_BYPASS_CHB_OFF) & ENABLE_PA_BYPASS_CHB_MSK)
#define SET_EXT_FE_CH4_400_7200_E_ENABLE_PA_BYPASS_CHA(p, f) (p) = ((p) & ~ENABLE_PA_BYPASS_CHA_MSK) | (((f) << ENABLE_PA_BYPASS_CHA_OFF) & ENABLE_PA_BYPASS_CHA_MSK)
#define SET_EXT_FE_CH4_400_7200_E_ENABLE_IF_VBYP(p, f) (p) = ((p) & ~ENABLE_IF_VBYP_MSK) | (((f) << ENABLE_IF_VBYP_OFF) & ENABLE_IF_VBYP_MSK)
#define SET_EXT_FE_CH4_400_7200_E_ENABLE_REF_GPS(p, f) (p) = ((p) & ~ENABLE_REF_GPS_MSK) | (((f) << ENABLE_REF_GPS_OFF) & ENABLE_REF_GPS_MSK)
#define SET_EXT_FE_CH4_400_7200_E_ENABLE_P8V_TX(p, f) (p) = ((p) & ~ENABLE_P8V_TX_MSK) | (((f) << ENABLE_P8V_TX_OFF) & ENABLE_P8V_TX_MSK)
#define SET_EXT_FE_CH4_400_7200_E_ENABLE_P6V_RX(p, f) (p) = ((p) & ~ENABLE_P6V_RX_MSK) | (((f) << ENABLE_P6V_RX_OFF) & ENABLE_P6V_RX_MSK)

#define MAKE_EXT_FE_CH4_400_7200_E_ENABLE(pa_bypass_chd, pa_bypass_chc, pa_bypass_chb, pa_bypass_cha, if_vbyp, ref_gps, p8v_tx, p6v_rx) MAKE_EXT_FE_CH4_400_7200_E_REG_WR(ENABLE, \
    (((pa_bypass_chd) << ENABLE_PA_BYPASS_CHD_OFF) & ENABLE_PA_BYPASS_CHD_MSK) |  \
    (((pa_bypass_chc) << ENABLE_PA_BYPASS_CHC_OFF) & ENABLE_PA_BYPASS_CHC_MSK) |  \
    (((pa_bypass_chb) << ENABLE_PA_BYPASS_CHB_OFF) & ENABLE_PA_BYPASS_CHB_MSK) |  \
    (((pa_bypass_cha) << ENABLE_PA_BYPASS_CHA_OFF) & ENABLE_PA_BYPASS_CHA_MSK) |  \
    (((if_vbyp) << ENABLE_IF_VBYP_OFF) & ENABLE_IF_VBYP_MSK) |  \
    (((ref_gps) << ENABLE_REF_GPS_OFF) & ENABLE_REF_GPS_MSK) |  \
    (((p8v_tx) << ENABLE_P8V_TX_OFF) & ENABLE_P8V_TX_MSK) |  \
    (((p6v_rx) << ENABLE_P6V_RX_OFF) & ENABLE_P6V_RX_MSK))
// Register R50 [0x32] -- LED_TRX_CTRL

enum led_trx_ctrl_fields_t {
    LED_TRX_CTRL_LED_CHD_OFF = 0x6,
    LED_TRX_CTRL_LED_CHD_MSK = 0xc0,
    LED_TRX_CTRL_LED_CHC_OFF = 0x4,
    LED_TRX_CTRL_LED_CHC_MSK = 0x30,
    LED_TRX_CTRL_LED_CHB_OFF = 0x2,
    LED_TRX_CTRL_LED_CHB_MSK = 0xc,
    LED_TRX_CTRL_LED_CHA_OFF = 0x0,
    LED_TRX_CTRL_LED_CHA_MSK = 0x3,
};
#define GET_EXT_FE_CH4_400_7200_E_LED_TRX_CTRL_LED_CHD(x) (((x) & LED_TRX_CTRL_LED_CHD_MSK) >> LED_TRX_CTRL_LED_CHD_OFF)
#define GET_EXT_FE_CH4_400_7200_E_LED_TRX_CTRL_LED_CHC(x) (((x) & LED_TRX_CTRL_LED_CHC_MSK) >> LED_TRX_CTRL_LED_CHC_OFF)
#define GET_EXT_FE_CH4_400_7200_E_LED_TRX_CTRL_LED_CHB(x) (((x) & LED_TRX_CTRL_LED_CHB_MSK) >> LED_TRX_CTRL_LED_CHB_OFF)
#define GET_EXT_FE_CH4_400_7200_E_LED_TRX_CTRL_LED_CHA(x) (((x) & LED_TRX_CTRL_LED_CHA_MSK) >> LED_TRX_CTRL_LED_CHA_OFF)
#define SET_EXT_FE_CH4_400_7200_E_LED_TRX_CTRL_LED_CHD(p, f) (p) = ((p) & ~LED_TRX_CTRL_LED_CHD_MSK) | (((f) << LED_TRX_CTRL_LED_CHD_OFF) & LED_TRX_CTRL_LED_CHD_MSK)
#define SET_EXT_FE_CH4_400_7200_E_LED_TRX_CTRL_LED_CHC(p, f) (p) = ((p) & ~LED_TRX_CTRL_LED_CHC_MSK) | (((f) << LED_TRX_CTRL_LED_CHC_OFF) & LED_TRX_CTRL_LED_CHC_MSK)
#define SET_EXT_FE_CH4_400_7200_E_LED_TRX_CTRL_LED_CHB(p, f) (p) = ((p) & ~LED_TRX_CTRL_LED_CHB_MSK) | (((f) << LED_TRX_CTRL_LED_CHB_OFF) & LED_TRX_CTRL_LED_CHB_MSK)
#define SET_EXT_FE_CH4_400_7200_E_LED_TRX_CTRL_LED_CHA(p, f) (p) = ((p) & ~LED_TRX_CTRL_LED_CHA_MSK) | (((f) << LED_TRX_CTRL_LED_CHA_OFF) & LED_TRX_CTRL_LED_CHA_MSK)

#define MAKE_EXT_FE_CH4_400_7200_E_LED_TRX_CTRL(led_chd, led_chc, led_chb, led_cha) MAKE_EXT_FE_CH4_400_7200_E_REG_WR(LED_TRX_CTRL, \
    (((led_chd) << LED_TRX_CTRL_LED_CHD_OFF) & LED_TRX_CTRL_LED_CHD_MSK) |  \
    (((led_chc) << LED_TRX_CTRL_LED_CHC_OFF) & LED_TRX_CTRL_LED_CHC_MSK) |  \
    (((led_chb) << LED_TRX_CTRL_LED_CHB_OFF) & LED_TRX_CTRL_LED_CHB_MSK) |  \
    (((led_cha) << LED_TRX_CTRL_LED_CHA_OFF) & LED_TRX_CTRL_LED_CHA_MSK))
// Register R51 [0x33] -- LEDRX_CH_CTRL

enum ledrx_ch_ctrl_fields_t {
    LEDRX_CH_CTRL_LED_CHD_OFF = 0x7,
    LEDRX_CH_CTRL_LED_CHD_MSK = 0x80,
    LEDRX_CH_CTRL_LED_CHC_OFF = 0x6,
    LEDRX_CH_CTRL_LED_CHC_MSK = 0x40,
    LEDRX_CH_CTRL_LED_CHB_OFF = 0x5,
    LEDRX_CH_CTRL_LED_CHB_MSK = 0x20,
    LEDRX_CH_CTRL_LED_CHA_OFF = 0x4,
    LEDRX_CH_CTRL_LED_CHA_MSK = 0x10,
    LEDRX_CH_CTRL_EN_CHD_OFF = 0x3,
    LEDRX_CH_CTRL_EN_CHD_MSK = 0x8,
    LEDRX_CH_CTRL_EN_CHC_OFF = 0x2,
    LEDRX_CH_CTRL_EN_CHC_MSK = 0x4,
    LEDRX_CH_CTRL_EN_CHB_OFF = 0x1,
    LEDRX_CH_CTRL_EN_CHB_MSK = 0x2,
    LEDRX_CH_CTRL_EN_CHA_OFF = 0x0,
    LEDRX_CH_CTRL_EN_CHA_MSK = 0x1,
};
#define GET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_LED_CHD(x) (((x) & LEDRX_CH_CTRL_LED_CHD_MSK) >> LEDRX_CH_CTRL_LED_CHD_OFF)
#define GET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_LED_CHC(x) (((x) & LEDRX_CH_CTRL_LED_CHC_MSK) >> LEDRX_CH_CTRL_LED_CHC_OFF)
#define GET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_LED_CHB(x) (((x) & LEDRX_CH_CTRL_LED_CHB_MSK) >> LEDRX_CH_CTRL_LED_CHB_OFF)
#define GET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_LED_CHA(x) (((x) & LEDRX_CH_CTRL_LED_CHA_MSK) >> LEDRX_CH_CTRL_LED_CHA_OFF)
#define GET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_EN_CHD(x) (((x) & LEDRX_CH_CTRL_EN_CHD_MSK) >> LEDRX_CH_CTRL_EN_CHD_OFF)
#define GET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_EN_CHC(x) (((x) & LEDRX_CH_CTRL_EN_CHC_MSK) >> LEDRX_CH_CTRL_EN_CHC_OFF)
#define GET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_EN_CHB(x) (((x) & LEDRX_CH_CTRL_EN_CHB_MSK) >> LEDRX_CH_CTRL_EN_CHB_OFF)
#define GET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_EN_CHA(x) (((x) & LEDRX_CH_CTRL_EN_CHA_MSK) >> LEDRX_CH_CTRL_EN_CHA_OFF)
#define SET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_LED_CHD(p, f) (p) = ((p) & ~LEDRX_CH_CTRL_LED_CHD_MSK) | (((f) << LEDRX_CH_CTRL_LED_CHD_OFF) & LEDRX_CH_CTRL_LED_CHD_MSK)
#define SET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_LED_CHC(p, f) (p) = ((p) & ~LEDRX_CH_CTRL_LED_CHC_MSK) | (((f) << LEDRX_CH_CTRL_LED_CHC_OFF) & LEDRX_CH_CTRL_LED_CHC_MSK)
#define SET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_LED_CHB(p, f) (p) = ((p) & ~LEDRX_CH_CTRL_LED_CHB_MSK) | (((f) << LEDRX_CH_CTRL_LED_CHB_OFF) & LEDRX_CH_CTRL_LED_CHB_MSK)
#define SET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_LED_CHA(p, f) (p) = ((p) & ~LEDRX_CH_CTRL_LED_CHA_MSK) | (((f) << LEDRX_CH_CTRL_LED_CHA_OFF) & LEDRX_CH_CTRL_LED_CHA_MSK)
#define SET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_EN_CHD(p, f) (p) = ((p) & ~LEDRX_CH_CTRL_EN_CHD_MSK) | (((f) << LEDRX_CH_CTRL_EN_CHD_OFF) & LEDRX_CH_CTRL_EN_CHD_MSK)
#define SET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_EN_CHC(p, f) (p) = ((p) & ~LEDRX_CH_CTRL_EN_CHC_MSK) | (((f) << LEDRX_CH_CTRL_EN_CHC_OFF) & LEDRX_CH_CTRL_EN_CHC_MSK)
#define SET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_EN_CHB(p, f) (p) = ((p) & ~LEDRX_CH_CTRL_EN_CHB_MSK) | (((f) << LEDRX_CH_CTRL_EN_CHB_OFF) & LEDRX_CH_CTRL_EN_CHB_MSK)
#define SET_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL_EN_CHA(p, f) (p) = ((p) & ~LEDRX_CH_CTRL_EN_CHA_MSK) | (((f) << LEDRX_CH_CTRL_EN_CHA_OFF) & LEDRX_CH_CTRL_EN_CHA_MSK)

#define MAKE_EXT_FE_CH4_400_7200_E_LEDRX_CH_CTRL(led_chd, led_chc, led_chb, led_cha, en_chd, en_chc, en_chb, en_cha) MAKE_EXT_FE_CH4_400_7200_E_REG_WR(LEDRX_CH_CTRL, \
    (((led_chd) << LEDRX_CH_CTRL_LED_CHD_OFF) & LEDRX_CH_CTRL_LED_CHD_MSK) |  \
    (((led_chc) << LEDRX_CH_CTRL_LED_CHC_OFF) & LEDRX_CH_CTRL_LED_CHC_MSK) |  \
    (((led_chb) << LEDRX_CH_CTRL_LED_CHB_OFF) & LEDRX_CH_CTRL_LED_CHB_MSK) |  \
    (((led_cha) << LEDRX_CH_CTRL_LED_CHA_OFF) & LEDRX_CH_CTRL_LED_CHA_MSK) |  \
    (((en_chd) << LEDRX_CH_CTRL_EN_CHD_OFF) & LEDRX_CH_CTRL_EN_CHD_MSK) |  \
    (((en_chc) << LEDRX_CH_CTRL_EN_CHC_OFF) & LEDRX_CH_CTRL_EN_CHC_MSK) |  \
    (((en_chb) << LEDRX_CH_CTRL_EN_CHB_OFF) & LEDRX_CH_CTRL_EN_CHB_MSK) |  \
    (((en_cha) << LEDRX_CH_CTRL_EN_CHA_OFF) & LEDRX_CH_CTRL_EN_CHA_MSK))
// Register R52 [0x34] -- P_A_EN_AB

enum p_a_en_ab_fields_t {
    P_A_EN_AB_A_OFF = 0x7,
    P_A_EN_AB_A_MSK = 0x80,
    P_A_EN_AB_B_OFF = 0x6,
    P_A_EN_AB_B_MSK = 0x40,
};
#define GET_EXT_FE_CH4_400_7200_E_P_A_EN_AB_A(x) (((x) & P_A_EN_AB_A_MSK) >> P_A_EN_AB_A_OFF)
#define GET_EXT_FE_CH4_400_7200_E_P_A_EN_AB_B(x) (((x) & P_A_EN_AB_B_MSK) >> P_A_EN_AB_B_OFF)
#define SET_EXT_FE_CH4_400_7200_E_P_A_EN_AB_A(p, f) (p) = ((p) & ~P_A_EN_AB_A_MSK) | (((f) << P_A_EN_AB_A_OFF) & P_A_EN_AB_A_MSK)
#define SET_EXT_FE_CH4_400_7200_E_P_A_EN_AB_B(p, f) (p) = ((p) & ~P_A_EN_AB_B_MSK) | (((f) << P_A_EN_AB_B_OFF) & P_A_EN_AB_B_MSK)

#define MAKE_EXT_FE_CH4_400_7200_E_P_A_EN_AB(a, b) MAKE_EXT_FE_CH4_400_7200_E_REG_WR(P_A_EN_AB, \
    (((a) << P_A_EN_AB_A_OFF) & P_A_EN_AB_A_MSK) |  \
    (((b) << P_A_EN_AB_B_OFF) & P_A_EN_AB_B_MSK))
// Register R53 [0x35] -- ATTN_RX_CH_AB

enum attn_rx_ch_ab_fields_t {
    ATTN_RX_CH_AB_A_OFF = 0x4,
    ATTN_RX_CH_AB_A_MSK = 0xf0,
    ATTN_RX_CH_AB_B_OFF = 0x0,
    ATTN_RX_CH_AB_B_MSK = 0xf,
};
#define GET_EXT_FE_CH4_400_7200_E_ATTN_RX_CH_AB_A(x) (((x) & ATTN_RX_CH_AB_A_MSK) >> ATTN_RX_CH_AB_A_OFF)
#define GET_EXT_FE_CH4_400_7200_E_ATTN_RX_CH_AB_B(x) (((x) & ATTN_RX_CH_AB_B_MSK) >> ATTN_RX_CH_AB_B_OFF)
#define SET_EXT_FE_CH4_400_7200_E_ATTN_RX_CH_AB_A(p, f) (p) = ((p) & ~ATTN_RX_CH_AB_A_MSK) | (((f) << ATTN_RX_CH_AB_A_OFF) & ATTN_RX_CH_AB_A_MSK)
#define SET_EXT_FE_CH4_400_7200_E_ATTN_RX_CH_AB_B(p, f) (p) = ((p) & ~ATTN_RX_CH_AB_B_MSK) | (((f) << ATTN_RX_CH_AB_B_OFF) & ATTN_RX_CH_AB_B_MSK)

#define MAKE_EXT_FE_CH4_400_7200_E_ATTN_RX_CH_AB(a, b) MAKE_EXT_FE_CH4_400_7200_E_REG_WR(ATTN_RX_CH_AB, \
    (((a) << ATTN_RX_CH_AB_A_OFF) & ATTN_RX_CH_AB_A_MSK) |  \
    (((b) << ATTN_RX_CH_AB_B_OFF) & ATTN_RX_CH_AB_B_MSK))
// Register R54 [0x36] -- SW_AB

enum sw_ab_fields_t {
    SW_AB_RXTX_B_OFF = 0x6,
    SW_AB_RXTX_B_MSK = 0x40,
    SW_AB_PA_ON_B_OFF = 0x5,
    SW_AB_PA_ON_B_MSK = 0x20,
    SW_AB_RXTX_A_OFF = 0x3,
    SW_AB_RXTX_A_MSK = 0x8,
    SW_AB_PA_ON_A_OFF = 0x2,
    SW_AB_PA_ON_A_MSK = 0x4,
    SW_AB_TDDFDD_A_OFF = 0x1,
    SW_AB_TDDFDD_A_MSK = 0x12,
    SW_AB_TDDFDD_B_OFF = 0x0,
    SW_AB_TDDFDD_B_MSK = 0x81,
};
#define GET_EXT_FE_CH4_400_7200_E_SW_AB_RXTX_B(x) (((x) & SW_AB_RXTX_B_MSK) >> SW_AB_RXTX_B_OFF)
#define GET_EXT_FE_CH4_400_7200_E_SW_AB_PA_ON_B(x) (((x) & SW_AB_PA_ON_B_MSK) >> SW_AB_PA_ON_B_OFF)
#define GET_EXT_FE_CH4_400_7200_E_SW_AB_RXTX_A(x) (((x) & SW_AB_RXTX_A_MSK) >> SW_AB_RXTX_A_OFF)
#define GET_EXT_FE_CH4_400_7200_E_SW_AB_PA_ON_A(x) (((x) & SW_AB_PA_ON_A_MSK) >> SW_AB_PA_ON_A_OFF)
#define GET_EXT_FE_CH4_400_7200_E_SW_AB_TDDFDD_A(x) ((((((x) & SW_AB_TDDFDD_A_MSK) >> 1) & 0x1) << 0) | (((((x) & SW_AB_TDDFDD_A_MSK) >> 4) & 0x1) << 1))
#define GET_EXT_FE_CH4_400_7200_E_SW_AB_TDDFDD_B(x) ((((((x) & SW_AB_TDDFDD_B_MSK) >> 0) & 0x1) << 0) | (((((x) & SW_AB_TDDFDD_B_MSK) >> 7) & 0x1) << 1))
#define SET_EXT_FE_CH4_400_7200_E_SW_AB_RXTX_B(p, f) (p) = ((p) & ~SW_AB_RXTX_B_MSK) | (((f) << SW_AB_RXTX_B_OFF) & SW_AB_RXTX_B_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_AB_PA_ON_B(p, f) (p) = ((p) & ~SW_AB_PA_ON_B_MSK) | (((f) << SW_AB_PA_ON_B_OFF) & SW_AB_PA_ON_B_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_AB_RXTX_A(p, f) (p) = ((p) & ~SW_AB_RXTX_A_MSK) | (((f) << SW_AB_RXTX_A_OFF) & SW_AB_RXTX_A_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_AB_PA_ON_A(p, f) (p) = ((p) & ~SW_AB_PA_ON_A_MSK) | (((f) << SW_AB_PA_ON_A_OFF) & SW_AB_PA_ON_A_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_AB_TDDFDD_A(p, f) (p) = ((p) & ~SW_AB_TDDFDD_A_MSK) | ((((((f) >> 1) & 0x1) << 4) | ((((f) >> 0) & 0x1) << 1)) & SW_AB_TDDFDD_A_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_AB_TDDFDD_B(p, f) (p) = ((p) & ~SW_AB_TDDFDD_B_MSK) | ((((((f) >> 1) & 0x1) << 7) | ((((f) >> 0) & 0x1) << 0)) & SW_AB_TDDFDD_B_MSK)

#define MAKE_EXT_FE_CH4_400_7200_E_SW_AB(rxtx_b, pa_on_b, rxtx_a, pa_on_a, tddfdd_a, tddfdd_b) MAKE_EXT_FE_CH4_400_7200_E_REG_WR(SW_AB, \
    (((rxtx_b) << SW_AB_RXTX_B_OFF) & SW_AB_RXTX_B_MSK) |  \
    (((pa_on_b) << SW_AB_PA_ON_B_OFF) & SW_AB_PA_ON_B_MSK) |  \
    (((rxtx_a) << SW_AB_RXTX_A_OFF) & SW_AB_RXTX_A_MSK) |  \
    (((pa_on_a) << SW_AB_PA_ON_A_OFF) & SW_AB_PA_ON_A_MSK) |  \
    ((((((tddfdd_a) >> 1) & 0x1) << 4) | ((((tddfdd_a) >> 0) & 0x1) << 1)) & SW_AB_TDDFDD_A_MSK) |  \
    ((((((tddfdd_b) >> 1) & 0x1) << 7) | ((((tddfdd_b) >> 0) & 0x1) << 0)) & SW_AB_TDDFDD_B_MSK))
// Register R55 [0x37] -- P_A_EN_CD

enum p_a_en_cd_fields_t {
    P_A_EN_CD_C_OFF = 0x7,
    P_A_EN_CD_C_MSK = 0x80,
    P_A_EN_CD_D_OFF = 0x6,
    P_A_EN_CD_D_MSK = 0x40,
};
#define GET_EXT_FE_CH4_400_7200_E_P_A_EN_CD_C(x) (((x) & P_A_EN_CD_C_MSK) >> P_A_EN_CD_C_OFF)
#define GET_EXT_FE_CH4_400_7200_E_P_A_EN_CD_D(x) (((x) & P_A_EN_CD_D_MSK) >> P_A_EN_CD_D_OFF)
#define SET_EXT_FE_CH4_400_7200_E_P_A_EN_CD_C(p, f) (p) = ((p) & ~P_A_EN_CD_C_MSK) | (((f) << P_A_EN_CD_C_OFF) & P_A_EN_CD_C_MSK)
#define SET_EXT_FE_CH4_400_7200_E_P_A_EN_CD_D(p, f) (p) = ((p) & ~P_A_EN_CD_D_MSK) | (((f) << P_A_EN_CD_D_OFF) & P_A_EN_CD_D_MSK)

#define MAKE_EXT_FE_CH4_400_7200_E_P_A_EN_CD(c, d) MAKE_EXT_FE_CH4_400_7200_E_REG_WR(P_A_EN_CD, \
    (((c) << P_A_EN_CD_C_OFF) & P_A_EN_CD_C_MSK) |  \
    (((d) << P_A_EN_CD_D_OFF) & P_A_EN_CD_D_MSK))
// Register R56 [0x38] -- ATTN_RX_CH_CD

enum attn_rx_ch_cd_fields_t {
    ATTN_RX_CH_CD_C_OFF = 0x4,
    ATTN_RX_CH_CD_C_MSK = 0xf0,
    ATTN_RX_CH_CD_D_OFF = 0x0,
    ATTN_RX_CH_CD_D_MSK = 0xf,
};
#define GET_EXT_FE_CH4_400_7200_E_ATTN_RX_CH_CD_C(x) (((x) & ATTN_RX_CH_CD_C_MSK) >> ATTN_RX_CH_CD_C_OFF)
#define GET_EXT_FE_CH4_400_7200_E_ATTN_RX_CH_CD_D(x) (((x) & ATTN_RX_CH_CD_D_MSK) >> ATTN_RX_CH_CD_D_OFF)
#define SET_EXT_FE_CH4_400_7200_E_ATTN_RX_CH_CD_C(p, f) (p) = ((p) & ~ATTN_RX_CH_CD_C_MSK) | (((f) << ATTN_RX_CH_CD_C_OFF) & ATTN_RX_CH_CD_C_MSK)
#define SET_EXT_FE_CH4_400_7200_E_ATTN_RX_CH_CD_D(p, f) (p) = ((p) & ~ATTN_RX_CH_CD_D_MSK) | (((f) << ATTN_RX_CH_CD_D_OFF) & ATTN_RX_CH_CD_D_MSK)

#define MAKE_EXT_FE_CH4_400_7200_E_ATTN_RX_CH_CD(c, d) MAKE_EXT_FE_CH4_400_7200_E_REG_WR(ATTN_RX_CH_CD, \
    (((c) << ATTN_RX_CH_CD_C_OFF) & ATTN_RX_CH_CD_C_MSK) |  \
    (((d) << ATTN_RX_CH_CD_D_OFF) & ATTN_RX_CH_CD_D_MSK))
// Register R57 [0x39] -- SW_CD

enum sw_cd_fields_t {
    SW_CD_RXTX_D_OFF = 0x6,
    SW_CD_RXTX_D_MSK = 0x40,
    SW_CD_PA_ON_D_OFF = 0x5,
    SW_CD_PA_ON_D_MSK = 0x20,
    SW_CD_RXTX_C_OFF = 0x3,
    SW_CD_RXTX_C_MSK = 0x8,
    SW_CD_PA_ON_C_OFF = 0x2,
    SW_CD_PA_ON_C_MSK = 0x4,
    SW_CD_TDDFDD_C_OFF = 0x1,
    SW_CD_TDDFDD_C_MSK = 0x12,
    SW_CD_TDDFDD_D_OFF = 0x0,
    SW_CD_TDDFDD_D_MSK = 0x81,
};
#define GET_EXT_FE_CH4_400_7200_E_SW_CD_RXTX_D(x) (((x) & SW_CD_RXTX_D_MSK) >> SW_CD_RXTX_D_OFF)
#define GET_EXT_FE_CH4_400_7200_E_SW_CD_PA_ON_D(x) (((x) & SW_CD_PA_ON_D_MSK) >> SW_CD_PA_ON_D_OFF)
#define GET_EXT_FE_CH4_400_7200_E_SW_CD_RXTX_C(x) (((x) & SW_CD_RXTX_C_MSK) >> SW_CD_RXTX_C_OFF)
#define GET_EXT_FE_CH4_400_7200_E_SW_CD_PA_ON_C(x) (((x) & SW_CD_PA_ON_C_MSK) >> SW_CD_PA_ON_C_OFF)
#define GET_EXT_FE_CH4_400_7200_E_SW_CD_TDDFDD_C(x) ((((((x) & SW_CD_TDDFDD_C_MSK) >> 1) & 0x1) << 0) | (((((x) & SW_CD_TDDFDD_C_MSK) >> 4) & 0x1) << 1))
#define GET_EXT_FE_CH4_400_7200_E_SW_CD_TDDFDD_D(x) ((((((x) & SW_CD_TDDFDD_D_MSK) >> 0) & 0x1) << 0) | (((((x) & SW_CD_TDDFDD_D_MSK) >> 7) & 0x1) << 1))
#define SET_EXT_FE_CH4_400_7200_E_SW_CD_RXTX_D(p, f) (p) = ((p) & ~SW_CD_RXTX_D_MSK) | (((f) << SW_CD_RXTX_D_OFF) & SW_CD_RXTX_D_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_CD_PA_ON_D(p, f) (p) = ((p) & ~SW_CD_PA_ON_D_MSK) | (((f) << SW_CD_PA_ON_D_OFF) & SW_CD_PA_ON_D_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_CD_RXTX_C(p, f) (p) = ((p) & ~SW_CD_RXTX_C_MSK) | (((f) << SW_CD_RXTX_C_OFF) & SW_CD_RXTX_C_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_CD_PA_ON_C(p, f) (p) = ((p) & ~SW_CD_PA_ON_C_MSK) | (((f) << SW_CD_PA_ON_C_OFF) & SW_CD_PA_ON_C_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_CD_TDDFDD_C(p, f) (p) = ((p) & ~SW_CD_TDDFDD_C_MSK) | ((((((f) >> 1) & 0x1) << 4) | ((((f) >> 0) & 0x1) << 1)) & SW_CD_TDDFDD_C_MSK)
#define SET_EXT_FE_CH4_400_7200_E_SW_CD_TDDFDD_D(p, f) (p) = ((p) & ~SW_CD_TDDFDD_D_MSK) | ((((((f) >> 1) & 0x1) << 7) | ((((f) >> 0) & 0x1) << 0)) & SW_CD_TDDFDD_D_MSK)

#define MAKE_EXT_FE_CH4_400_7200_E_SW_CD(rxtx_d, pa_on_d, rxtx_c, pa_on_c, tddfdd_c, tddfdd_d) MAKE_EXT_FE_CH4_400_7200_E_REG_WR(SW_CD, \
    (((rxtx_d) << SW_CD_RXTX_D_OFF) & SW_CD_RXTX_D_MSK) |  \
    (((pa_on_d) << SW_CD_PA_ON_D_OFF) & SW_CD_PA_ON_D_MSK) |  \
    (((rxtx_c) << SW_CD_RXTX_C_OFF) & SW_CD_RXTX_C_MSK) |  \
    (((pa_on_c) << SW_CD_PA_ON_C_OFF) & SW_CD_PA_ON_C_MSK) |  \
    ((((((tddfdd_c) >> 1) & 0x1) << 4) | ((((tddfdd_c) >> 0) & 0x1) << 1)) & SW_CD_TDDFDD_C_MSK) |  \
    ((((((tddfdd_d) >> 1) & 0x1) << 7) | ((((tddfdd_d) >> 0) & 0x1) << 0)) & SW_CD_TDDFDD_D_MSK))
