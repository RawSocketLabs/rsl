enum lmk05318_regs_t {
    DEV_CTL = 0xc,
    INT_LIVE0 = 0xd,
    INT_LIVE1 = 0xe,
    INT_MASK0 = 0xf,
    INT_MASK1 = 0x10,
    INT_FLAG_POL0 = 0x11,
    INT_FLAG_POL1 = 0x12,
    INT_FLAG0 = 0x13,
    INT_FLAG1 = 0x14,
    INTCTL = 0x15,
    STAT_POL = 0x16,
    MUTELVL1 = 0x17,
    MUTELVL2 = 0x18,
    OUT_MUTE = 0x19,
    DPLL_MUTE = 0x1d,
    GPIO_OUT = 0x24,
    SPARE_NVMBASE2_BY2 = 0x27,
    SPARE_NVMBASE2_BY1 = 0x28,
    XO_CLKCTL1 = 0x2a,
    XO_CLKCTL2 = 0x2b,
    XO_CONFIG = 0x2c,
    REF_CLKCTL1 = 0x2d,
    REF_CLKCTL2 = 0x2e,
    PLL_CLK_CFG = 0x2f,
    STAT0_SEL = 0x30,
    STAT1_SEL = 0x31,
    PWDN = 0x32,
    OUTCTL_0 = 0x33,
    OUTCTL_1 = 0x34,
    OUTDIV_0_1 = 0x35,
    OUTCTL_2 = 0x36,
    OUTCTL_3 = 0x37,
    OUTDIV_2_3 = 0x38,
    OUTCTL_4 = 0x39,
    OUTDIV_4 = 0x3a,
    OUTCTL_5 = 0x3b,
    OUTDIV_5 = 0x3c,
    OUTCTL_6 = 0x3d,
    OUTDIV_6 = 0x3e,
    OUTCTL_7 = 0x3f,
    OUTDIV_7_STG2_BY0 = 0x40,
    OUTDIV_7_STG2_BY1 = 0x41,
    OUTDIV_7_STG2_BY2 = 0x42,
    OUTDIV_7 = 0x43,
    PREDRIVER = 0x44,
    OUTSYNCCTL = 0x46,
    OUTSYNCEN = 0x47,
    PLL1_CTRL0 = 0x4a,
    PLL1_CALCTRL0 = 0x4f,
    BAW_LOCKDET_PPM_MAX_BY1 = 0x50,
    BAW_LOCKDET_PPM_MAX_BY0 = 0x51,
    BAW_LOCKDET_CNTSTRT_BY0 = 0x52,
    BAW_LOCKDET_CNTSTRT_BY1 = 0x53,
    BAW_LOCKDET_CNTSTRT_BY2 = 0x54,
    BAW_LOCKDET_CNTSTRT_BY3 = 0x55,
    BAW_LOCKDET_VCO_CNTSTRT_BY0 = 0x56,
    BAW_LOCKDET_VCO_CNTSTRT_BY1 = 0x57,
    BAW_LOCKDET_VCO_CNTSTRT_BY2 = 0x58,
    BAW_LOCKDET_VCO_CNTSTRT_BY3 = 0x59,
    BAW_UNLOCKDET_PPM_MAX_BY0 = 0x5a,
    BAW_UNLOCKDET_PPM_MAX_BY1 = 0x5b,
    BAW_UNLOCKDET_CNTSTRT_BY0 = 0x5c,
    BAW_UNLOCKDET_CNTSTRT_BY1 = 0x5d,
    BAW_UNLOCKDET_CNTSTRT_BY2 = 0x5e,
    BAW_UNLOCKDET_CNTSTRT_BY3 = 0x5f,
    BAW_UNLOCKDET_VCO_CNTSTRT_BY0 = 0x60,
    BAW_UNLOCKDET_VCO_CNTSTRT_BY1 = 0x61,
    BAW_UNLOCKDET_VCO_CNTSTRT_BY2 = 0x62,
    BAW_UNLOCKDET_VCO_CNTSTRT_BY3 = 0x63,
    PLL2_CTRL0 = 0x64,
    PLL2_CTRL1 = 0x65,
    PLL2_CTRL2 = 0x66,
    PLL2_CTRL4 = 0x68,
    PLL2_CALCTRL0 = 0x69,
    PLL1_NDIV_BY0 = 0x6c,
    PLL1_NDIV_BY1 = 0x6d,
    PLL1_NUM_BY0 = 0x6e,
    PLL1_NUM_BY1 = 0x6f,
    PLL1_NUM_BY2 = 0x70,
    PLL1_NUM_BY3 = 0x71,
    PLL1_NUM_BY4 = 0x72,
    PLL1_MASHCTRL = 0x73,
    PLL1_MODE = 0x74,
    PLL1_NUM_STAT_BY0 = 0x7b,
    PLL1_NUM_STAT_BY1 = 0x7c,
    PLL1_NUM_STAT_BY2 = 0x7d,
    PLL1_NUM_STAT_BY3 = 0x7e,
    PLL1_NUM_STAT_BY4 = 0x7f,
    PLL1_LF_R2 = 0x81,
    PLL1_LF_C1 = 0x82,
    PLL1_LF_R3 = 0x83,
    PLL1_LF_R4 = 0x84,
    PLL2_NDIV_BY0 = 0x86,
    PLL2_NDIV_BY1 = 0x87,
    PLL2_NUM_BY0 = 0x88,
    PLL2_NUM_BY1 = 0x89,
    PLL2_NUM_BY2 = 0x8a,
    PLL2_MASHCTRL = 0x8b,
    PLL2_LF_R2 = 0x8c,
    PLL2_LF_R3 = 0x8e,
    PLL2_LF_R4 = 0x8f,
    PLL2_LF_C3C4 = 0x90,
    XO_OFFSET_SW_TIMER = 0x91,
    DPLL_TUNING_FREE_RUN_BY0 = 0xb4,
    DPLL_TUNING_FREE_RUN_BY1 = 0xb5,
    DPLL_TUNING_FREE_RUN_BY2 = 0xb6,
    DPLL_TUNING_FREE_RUN_BY3 = 0xb7,
    DPLL_TUNING_FREE_RUN_BY4 = 0xb8,
    DPLL_REF_HISTCTL = 0xb9,
    DPLL_REF_HISTCNT = 0xba,
    DPLL_REF_HISTDLY_BY0 = 0xbb,
    DPLL_REF_HISTDLY_BY1 = 0xbc,
    DPLL_REF_HISTDLY_BY2 = 0xbd,
    DPLL_REF_HISTDLY_BY3 = 0xbe,
    REF01_DETAMP = 0xc0,
    REF0_DETEN = 0xc1,
    REF1_DETEN = 0xc2,
    REF0_MISSCLK_DIV_BY0 = 0xc3,
    REF0_MISSCLK_DIV_BY1 = 0xc4,
    REF0_MISSCLK_DIV_BY2 = 0xc5,
    REF1_MISSCLK_DIV_BY0 = 0xc6,
    REF1_MISSCLK_DIV_BY1 = 0xc7,
    REF1_MISSCLK_DIV_BY2 = 0xc8,
    REF_MISSCLK_CTL = 0xc9,
    REF0_EARLY_CLK_DIV_BY0 = 0xca,
    REF0_EARLY_CLK_DIV_BY1 = 0xcb,
    REF0_EARLY_CLK_DIV_BY2 = 0xcc,
    REF1_EARLY_CLK_DIV_BY0 = 0xcd,
    REF1_EARLY_CLK_DIV_BY1 = 0xce,
    REF1_EARLY_CLK_DIV_BY2 = 0xcf,
    REF0_PPM_MIN_BY0 = 0xd0,
    REF0_PPM_MIN_BY1 = 0xd1,
    REF0_PPM_MAX_BY0 = 0xd2,
    REF0_PPM_MAX_BY1 = 0xd3,
    REF1_PPM_MIN_BY0 = 0xd4,
    REF1_PPM_MIN_BY1 = 0xd5,
    REF1_PPM_MAX_BY0 = 0xd6,
    REF1_PPM_MAX_BY1 = 0xd7,
    REF0_CNTSTRT_BY0 = 0xd9,
    REF0_CNTSTRT_BY1 = 0xda,
    REF0_CNTSTRT_BY2 = 0xdb,
    REF0_CNTSTRT_BY3 = 0xdc,
    REF0_HOLD_CNTSTRT_BY0 = 0xdd,
    REF0_HOLD_CNTSTRT_BY1 = 0xde,
    REF0_HOLD_CNTSTRT_BY2 = 0xdf,
    REF0_HOLD_CNTSTRT_BY3 = 0xe0,
    REF1_CNTSTRT_BY0 = 0xe1,
    REF1_CNTSTRT_BY1 = 0xe2,
    REF1_CNTSTRT_BY2 = 0xe3,
    REF1_CNTSTRT_BY3 = 0xe4,
    REF1_HOLD_CNTSTRT_BY0 = 0xe5,
    REF1_HOLD_CNTSTRT_BY1 = 0xe6,
    REF1_HOLD_CNTSTRT_BY2 = 0xe7,
    REF1_HOLD_CNTSTRT_BY3 = 0xe8,
    REF0_VLDTMR = 0xe9,
    REF1_VLDTMR = 0xea,
    REF0_PH_VALID_CNT_BY0 = 0xeb,
    REF0_PH_VALID_CNT_BY1 = 0xec,
    REF0_PH_VALID_CNT_BY2 = 0xed,
    REF0_PH_VALID_CNT_BY3 = 0xee,
    REF1_PH_VALID_CNT_BY0 = 0xef,
    REF1_PH_VALID_CNT_BY1 = 0xf0,
    REF1_PH_VALID_CNT_BY2 = 0xf1,
    REF1_PH_VALID_CNT_BY3 = 0xf2,
    REF0_PH_VALID_THR = 0xf3,
    REF1_PH_VALID_THR = 0xf4,
    DPLL_REF01_PRTY = 0xf9,
    DPLL_REF_SWMODE = 0xfb,
    DPLL_GEN_CTL = 0xfc,
    DPLL_SWITCHOVER_TMR_EXP = 0xfd,
    DPLL_SWITCHOVER_TMR_MANT_BY1 = 0xfe,
    DPLL_SWITCHOVER_TMR_MANT_BY0 = 0xff,
    DPLL_REF0_RDIV_BY0 = 0x100,
    DPLL_REF0_RDIV_BY1 = 0x101,
    DPLL_REF1_RDIV_BY0 = 0x102,
    DPLL_REF1_RDIV_BY1 = 0x103,
    DPLL_REF_TDC_CTL = 0x104,
    DPLL_REF_DLY_GEN = 0x105,
    DPLL_REF_CYCSLIP_OFFSET_BY0 = 0x106,
    DPLL_REF_CYCSLIP_OFFSET_BY1 = 0x107,
    DPLL_REF_CYCSLIP_OFFSET_BY2 = 0x108,
    DPLL_REF_CYCSLIP_OFFSET_BY3 = 0x109,
    DPLL_REF_CYCSLIP_OFFSET_BY4 = 0x10a,
    DPLL_REF_LOOPCTL = 0x10b,
    DPLL_REF_LOOPCTL_CHG = 0x10c,
    DPLL_REF_DECIMATION = 0x10d,
    DPLL_REF_FILTSCALAR_BY0 = 0x10e,
    DPLL_REF_FILTSCALAR_BY1 = 0x10f,
    DPLL_REF_FILTGAIN = 0x110,
    DPLL_REF_FILTGAIN_FL1 = 0x111,
    DPLL_REF_FILTGAIN_FL2 = 0x112,
    DPLL_REF_LOOPGAIN = 0x113,
    DPLL_REF_LOOPGAIN_FL1 = 0x114,
    DPLL_REF_LOOPGAIN_FL2 = 0x115,
    DPLL_REF_LPF0GAIN = 0x116,
    DPLL_REF_LPF0GAIN_FL1 = 0x117,
    DPLL_REF_LPF0GAIN_FL2 = 0x118,
    DPLL_REF_LPF1GAIN = 0x119,
    DPLL_REF_LPF1GAIN_FL1 = 0x11a,
    DPLL_REF_LPF1GAIN_FL2 = 0x11b,
    DPLL_REF_LPF0GAIN2_FL = 0x11c,
    DPLL_REF_LPF1GAIN2_FL = 0x11d,
    DPLL_REF_TMR_FL1_BY0 = 0x11e,
    DPLL_REF_TMR_FL1_BY1 = 0x11f,
    DPLL_REF_TMR_FL2_BY0 = 0x120,
    DPLL_REF_TMR_FL2_BY1 = 0x121,
    DPLL_REF_TMR_LCK_BY0 = 0x122,
    DPLL_REF_TMR_LCK_BY1 = 0x123,
    DPLL_REF_PHC_LPF = 0x124,
    DPLL_REF_PHC_CTRL = 0x125,
    DPLL_REF_PHC_TIMER_BY0 = 0x126,
    DPLL_REF_PHC_TIMER_BY1 = 0x127,
    DPLL_REF_QUANT = 0x128,
    DPLL_REF_QUANT_FL1 = 0x129,
    DPLL_REF_QUANT_FL2 = 0x12a,
    DPLL_PL_LPF_GAIN = 0x12c,
    DPLL_PL_THRESH = 0x12d,
    DPLL_PL_UNLK_THRESH = 0x12e,
    DPLL_REF_FB_PREDIV = 0x130,
    DPLL_REF_FB_DIV_BY0 = 0x131,
    DPLL_REF_FB_DIV_BY1 = 0x132,
    DPLL_REF_FB_DIV_BY2 = 0x133,
    DPLL_REF_FB_DIV_BY3 = 0x134,
    DPLL_REF_NUM_BY0 = 0x135,
    DPLL_REF_NUM_BY1 = 0x136,
    DPLL_REF_NUM_BY2 = 0x137,
    DPLL_REF_NUM_BY3 = 0x138,
    DPLL_REF_NUM_BY4 = 0x139,
    DPLL_REF_DEN_BY0 = 0x13a,
    DPLL_REF_DEN_BY1 = 0x13b,
    DPLL_REF_DEN_BY2 = 0x13c,
    DPLL_REF_DEN_BY3 = 0x13d,
    DPLL_REF_DEN_BY4 = 0x13e,
    DPLL_REF_MASHCTL = 0x13f,
    DPLL_REF_LOCKDET_1_5_BY0 = 0x140,
    DPLL_REF_LOCKDET_1_5_BY1 = 0x141,
    DPLL_REF_LOCKDET_1_5_BY2 = 0x142,
    DPLL_REF_LOCKDET_1_5_BY3 = 0x143,
    DPLL_REF_LOCKDET_1_5_BY4 = 0x144,
    DPLL_REF_LOCKDET_6_10_BY0 = 0x145,
    DPLL_REF_LOCKDET_6_10_BY1 = 0x146,
    DPLL_REF_LOCKDET_6_10_BY2 = 0x147,
    DPLL_REF_LOCKDET_6_10_BY3 = 0x148,
    DPLL_REF_LOCKDET_6_10_BY4 = 0x149,
    DPLL_REF_UNLOCKDET_1_3_BY0 = 0x14a,
    DPLL_REF_UNLOCKDET_1_3_BY1 = 0x14b,
    DPLL_REF_UNLOCKDET_1_3_BY2 = 0x14c,
    PLL2_DEN_BY0 = 0x14d,
    PLL2_DEN_BY1 = 0x14e,
    PLL2_DEN_BY2 = 0x14f,
    DPLL_REF_UNLOCKDET_VCO_CNTSTRT_BY0 = 0x150,
    DPLL_REF_UNLOCKDET_VCO_CNTSTRT_BY1 = 0x151,
    DPLL_REF_UNLOCKDET_VCO_CNTSTRT_BY2 = 0x152,
    PLL1_24B_NUM_23_16 = 0x153,
    DPLL_REF_SYNC_PH_OFFSET_BY0 = 0x154,
    DPLL_REF_SYNC_PH_OFFSET_BY1 = 0x155,
    DPLL_REF_SYNC_PH_OFFSET_BY2 = 0x156,
    DPLL_REF_SYNC_PH_OFFSET_BY3 = 0x157,
    DPLL_REF_SYNC_PH_OFFSET_BY4 = 0x158,
    DPLL_REF_SYNC_PH_OFFSET_BY5 = 0x159,
    DPLL_FDEV_CTL = 0x15a,
    DPLL_FDEV_BY0 = 0x15b,
    DPLL_FDEV_BY1 = 0x15c,
    DPLL_FDEV_BY2 = 0x15d,
    DPLL_FDEV_BY3 = 0x15e,
    DPLL_FDEV_BY4 = 0x15f,
    DPLL_FDEV_REG_CTL = 0x160,
    PLL1_CALSTAT1 = 0x165,
    PLL2_CALSTAT1 = 0x16f,
    REFVALSTAT = 0x19b,
    NVMCNT = 0x9c,
    NVMCTL = 0x9d,
    MEMADR_BY0 = 0x9f,
    MEMADR_BY1 = 0xa0,
    NVMDAT = 0xa1,
    RAMDAT = 0xa2,
    NVMUNLK = 0xa4,
};
#define MAKE_LMK05318_REG_WR(a, v) (((a) << 8) | ((v) & 0xff))
#define MAKE_LMK05318_REG_RD(a) (0x800000 | ((a) << 8))
enum lmk05318_in_opts_t {
    IN_OPTS_DC_DIFF_EXT = 0x0,
    IN_OPTS_AC_DIFF_EXT = 0x1,
    IN_OPTS_AC_DIFF_INT_100 = 0x3,
    IN_OPTS_HCSL_INT_50 = 0x4,
    IN_OPTS_CMOS = 0x8,
    IN_OPTS_SE_INT_50 = 0xc,
};
enum lmk05318_out_opts_t {
    OUT_OPTS_Disabled = 0x0,
    OUT_OPTS_AC_LVDS = 0x10,
    OUT_OPTS_AC_CML = 0x14,
    OUT_OPTS_AC_LVPECL = 0x18,
    OUT_OPTS_HCSL_EXT_50 = 0x2c,
    OUT_OPTS_HCSL_INT_50 = 0x2d,
    OUT_OPTS_LVCMOS_HIZ_HIZ = 0x30,
    OUT_OPTS_LVCMOS_HIZ_N = 0x32,
    OUT_OPTS_LVCMOS_HIZ_P = 0x33,
    OUT_OPTS_LVCMOS_LOW_LOW = 0x35,
    OUT_OPTS_LVCMOS_N_HIZ = 0x38,
    OUT_OPTS_LVCMOS_N_N = 0x3a,
    OUT_OPTS_LVCMOS_N_P = 0x3b,
    OUT_OPTS_LVCMOS_P_HIZ = 0x3c,
    OUT_OPTS_LVCMOS_P_N = 0x3e,
    OUT_OPTS_LVCMOS_P_P = 0x3f,
};
enum lmk05318_out_pll_sel_t {
    OUT_PLL_SEL_APLL1_P1 = 0x0,
    OUT_PLL_SEL_APLL1_P1_INV = 0x1,
    OUT_PLL_SEL_APLL2_P1 = 0x2,
    OUT_PLL_SEL_APLL2_P2 = 0x3,
};
enum lmk05318_mute_lvl_t {
    MUTE_LVL_BYPASS = 0x0,
    MUTE_LVL_DIFF_VOCM_P_BY_N_LOW = 0x1,
    MUTE_LVL_DIFF_HIGH_P_LOW_N_BY = 0x2,
    MUTE_LVL_DIFF_LOW_P_LOW_N_LOW = 0x3,
};
enum lmk05318_ref_dc_mode_t {
    REF_DC_MODE_AC_COUPLED_INT = 0x0,
    REF_DC_MODE_DC_COUPLED_INT = 0x1,
};
enum lmk05318_ref_buf_mode_t {
    REF_BUF_MODE_AC_HYST50_DC_EN = 0x0,
    REF_BUF_MODE_AC_HYST200_DC_DIS = 0x1,
};
enum lmk05318_ref_input_type_t {
    REF_INPUT_TYPE_DIFF_NOTERM = 0x1,
    REF_INPUT_TYPE_DIFF_100 = 0x3,
    REF_INPUT_TYPE_DIFF_50 = 0x5,
    REF_INPUT_TYPE_SE_NOTERM = 0x8,
    REF_INPUT_TYPE_SE_50 = 0xc,
};
// Register R12 [0xc] -- DEV_CTL

enum dev_ctl_fields_t {
    RESET_SW_OFF = 0x7,
    RESET_SW_MSK = 0x80,
    SYNC_SW_OFF = 0x6,
    SYNC_SW_MSK = 0x40,
    SYNC_AUTO_DPLL_OFF = 0x5,
    SYNC_AUTO_DPLL_MSK = 0x20,
    SYNC_AUTO_APLL_OFF = 0x4,
    SYNC_AUTO_APLL_MSK = 0x10,
    SYNC_MUTE_OFF = 0x3,
    SYNC_MUTE_MSK = 0x8,
    PLLSTRTMODE_OFF = 0x1,
    PLLSTRTMODE_MSK = 0x2,
    AUTOSTRT_OFF = 0x0,
    AUTOSTRT_MSK = 0x1,
};
#define MAKE_LMK05318_DEV_CTL(reset_sw, sync_sw, sync_auto_dpll, sync_auto_apll, sync_mute, pllstrtmode, autostrt) MAKE_LMK05318_REG_WR(DEV_CTL, \
    (((reset_sw) << RESET_SW_OFF) & RESET_SW_MSK) |  \
    (((sync_sw) << SYNC_SW_OFF) & SYNC_SW_MSK) |  \
    (((sync_auto_dpll) << SYNC_AUTO_DPLL_OFF) & SYNC_AUTO_DPLL_MSK) |  \
    (((sync_auto_apll) << SYNC_AUTO_APLL_OFF) & SYNC_AUTO_APLL_MSK) |  \
    (((sync_mute) << SYNC_MUTE_OFF) & SYNC_MUTE_MSK) |  \
    (((pllstrtmode) << PLLSTRTMODE_OFF) & PLLSTRTMODE_MSK) |  \
    (((autostrt) << AUTOSTRT_OFF) & AUTOSTRT_MSK))
// Register R13 [0xd] -- INT_LIVE0

enum int_live0_fields_t {
    LOS_FDET_XO_OFF = 0x4,
    LOS_FDET_XO_MSK = 0x10,
    LOL_PLL2_OFF = 0x3,
    LOL_PLL2_MSK = 0x8,
    LOL_PLL1_OFF = 0x2,
    LOL_PLL1_MSK = 0x4,
    LOS_XO_OFF = 0x0,
    LOS_XO_MSK = 0x1,
};
#define MAKE_LMK05318_INT_LIVE0(los_fdet_xo, lol_pll2, lol_pll1, los_xo) MAKE_LMK05318_REG_WR(INT_LIVE0, \
    (((los_fdet_xo) << LOS_FDET_XO_OFF) & LOS_FDET_XO_MSK) |  \
    (((lol_pll2) << LOL_PLL2_OFF) & LOL_PLL2_MSK) |  \
    (((lol_pll1) << LOL_PLL1_OFF) & LOL_PLL1_MSK) |  \
    (((los_xo) << LOS_XO_OFF) & LOS_XO_MSK))
// Register R14 [0xe] -- INT_LIVE1

enum int_live1_fields_t {
    LOPL_DPLL_OFF = 0x7,
    LOPL_DPLL_MSK = 0x80,
    LOFL_DPLL_OFF = 0x6,
    LOFL_DPLL_MSK = 0x40,
    HIST_OFF = 0x5,
    HIST_MSK = 0x20,
    HLDOVR_OFF = 0x4,
    HLDOVR_MSK = 0x10,
    REFSWITCH_OFF = 0x3,
    REFSWITCH_MSK = 0x8,
    LOR_MISSCLK_OFF = 0x2,
    LOR_MISSCLK_MSK = 0x4,
    LOR_FREQ_OFF = 0x1,
    LOR_FREQ_MSK = 0x2,
    LOR_AMP_OFF = 0x0,
    LOR_AMP_MSK = 0x1,
};
#define MAKE_LMK05318_INT_LIVE1(lopl_dpll, lofl_dpll, hist, hldovr, refswitch, lor_missclk, lor_freq, lor_amp) MAKE_LMK05318_REG_WR(INT_LIVE1, \
    (((lopl_dpll) << LOPL_DPLL_OFF) & LOPL_DPLL_MSK) |  \
    (((lofl_dpll) << LOFL_DPLL_OFF) & LOFL_DPLL_MSK) |  \
    (((hist) << HIST_OFF) & HIST_MSK) |  \
    (((hldovr) << HLDOVR_OFF) & HLDOVR_MSK) |  \
    (((refswitch) << REFSWITCH_OFF) & REFSWITCH_MSK) |  \
    (((lor_missclk) << LOR_MISSCLK_OFF) & LOR_MISSCLK_MSK) |  \
    (((lor_freq) << LOR_FREQ_OFF) & LOR_FREQ_MSK) |  \
    (((lor_amp) << LOR_AMP_OFF) & LOR_AMP_MSK))
// Register R15 [0xf] -- INT_MASK0

enum int_mask0_fields_t {
    LOS_FDET_XO_MASK_OFF = 0x4,
    LOS_FDET_XO_MASK_MSK = 0x10,
    LOL_PLL2_MASK_OFF = 0x3,
    LOL_PLL2_MASK_MSK = 0x8,
    LOL_PLL1_MASK_OFF = 0x2,
    LOL_PLL1_MASK_MSK = 0x4,
    LOS_XO_MASK_OFF = 0x0,
    LOS_XO_MASK_MSK = 0x1,
};
#define MAKE_LMK05318_INT_MASK0(los_fdet_xo_mask, lol_pll2_mask, lol_pll1_mask, los_xo_mask) MAKE_LMK05318_REG_WR(INT_MASK0, \
    (((los_fdet_xo_mask) << LOS_FDET_XO_MASK_OFF) & LOS_FDET_XO_MASK_MSK) |  \
    (((lol_pll2_mask) << LOL_PLL2_MASK_OFF) & LOL_PLL2_MASK_MSK) |  \
    (((lol_pll1_mask) << LOL_PLL1_MASK_OFF) & LOL_PLL1_MASK_MSK) |  \
    (((los_xo_mask) << LOS_XO_MASK_OFF) & LOS_XO_MASK_MSK))
// Register R16 [0x10] -- INT_MASK1

enum int_mask1_fields_t {
    LOPL_DPLL_MASK_OFF = 0x7,
    LOPL_DPLL_MASK_MSK = 0x80,
    LOFL_DPLL_MASK_OFF = 0x6,
    LOFL_DPLL_MASK_MSK = 0x40,
    HIST_MASK_OFF = 0x5,
    HIST_MASK_MSK = 0x20,
    HLDOVR_MASK_OFF = 0x4,
    HLDOVR_MASK_MSK = 0x10,
    REFSWITCH_MASK_OFF = 0x3,
    REFSWITCH_MASK_MSK = 0x8,
    LOR_MISSCLK_MASK_OFF = 0x2,
    LOR_MISSCLK_MASK_MSK = 0x4,
    LOR_FREQ_MASK_OFF = 0x1,
    LOR_FREQ_MASK_MSK = 0x2,
    LOR_AMP_MASK_OFF = 0x0,
    LOR_AMP_MASK_MSK = 0x1,
};
#define MAKE_LMK05318_INT_MASK1(lopl_dpll_mask, lofl_dpll_mask, hist_mask, hldovr_mask, refswitch_mask, lor_missclk_mask, lor_freq_mask, lor_amp_mask) MAKE_LMK05318_REG_WR(INT_MASK1, \
    (((lopl_dpll_mask) << LOPL_DPLL_MASK_OFF) & LOPL_DPLL_MASK_MSK) |  \
    (((lofl_dpll_mask) << LOFL_DPLL_MASK_OFF) & LOFL_DPLL_MASK_MSK) |  \
    (((hist_mask) << HIST_MASK_OFF) & HIST_MASK_MSK) |  \
    (((hldovr_mask) << HLDOVR_MASK_OFF) & HLDOVR_MASK_MSK) |  \
    (((refswitch_mask) << REFSWITCH_MASK_OFF) & REFSWITCH_MASK_MSK) |  \
    (((lor_missclk_mask) << LOR_MISSCLK_MASK_OFF) & LOR_MISSCLK_MASK_MSK) |  \
    (((lor_freq_mask) << LOR_FREQ_MASK_OFF) & LOR_FREQ_MASK_MSK) |  \
    (((lor_amp_mask) << LOR_AMP_MASK_OFF) & LOR_AMP_MASK_MSK))
// Register R17 [0x11] -- INT_FLAG_POL0

enum int_flag_pol0_fields_t {
    LOS_FDET_XO_POL_OFF = 0x4,
    LOS_FDET_XO_POL_MSK = 0x10,
    LOL_PLL2_POL_OFF = 0x3,
    LOL_PLL2_POL_MSK = 0x8,
    LOL_PLL1_POL_OFF = 0x2,
    LOL_PLL1_POL_MSK = 0x4,
    LOS_XO_POL_OFF = 0x0,
    LOS_XO_POL_MSK = 0x1,
};
#define MAKE_LMK05318_INT_FLAG_POL0(los_fdet_xo_pol, lol_pll2_pol, lol_pll1_pol, los_xo_pol) MAKE_LMK05318_REG_WR(INT_FLAG_POL0, \
    (((los_fdet_xo_pol) << LOS_FDET_XO_POL_OFF) & LOS_FDET_XO_POL_MSK) |  \
    (((lol_pll2_pol) << LOL_PLL2_POL_OFF) & LOL_PLL2_POL_MSK) |  \
    (((lol_pll1_pol) << LOL_PLL1_POL_OFF) & LOL_PLL1_POL_MSK) |  \
    (((los_xo_pol) << LOS_XO_POL_OFF) & LOS_XO_POL_MSK))
// Register R18 [0x12] -- INT_FLAG_POL1

enum int_flag_pol1_fields_t {
    LOPL_DPLL_POL_OFF = 0x7,
    LOPL_DPLL_POL_MSK = 0x80,
    LOFL_DPLL_POL_OFF = 0x6,
    LOFL_DPLL_POL_MSK = 0x40,
    HIST_POL_OFF = 0x5,
    HIST_POL_MSK = 0x20,
    HLDOVR_POL_OFF = 0x4,
    HLDOVR_POL_MSK = 0x10,
    REFSWITCH_POL_OFF = 0x3,
    REFSWITCH_POL_MSK = 0x8,
    LOR_MISSCLK_POL_OFF = 0x2,
    LOR_MISSCLK_POL_MSK = 0x4,
    LOR_FREQ_POL_OFF = 0x1,
    LOR_FREQ_POL_MSK = 0x2,
    LOR_AMP_POL_OFF = 0x0,
    LOR_AMP_POL_MSK = 0x1,
};
#define MAKE_LMK05318_INT_FLAG_POL1(lopl_dpll_pol, lofl_dpll_pol, hist_pol, hldovr_pol, refswitch_pol, lor_missclk_pol, lor_freq_pol, lor_amp_pol) MAKE_LMK05318_REG_WR(INT_FLAG_POL1, \
    (((lopl_dpll_pol) << LOPL_DPLL_POL_OFF) & LOPL_DPLL_POL_MSK) |  \
    (((lofl_dpll_pol) << LOFL_DPLL_POL_OFF) & LOFL_DPLL_POL_MSK) |  \
    (((hist_pol) << HIST_POL_OFF) & HIST_POL_MSK) |  \
    (((hldovr_pol) << HLDOVR_POL_OFF) & HLDOVR_POL_MSK) |  \
    (((refswitch_pol) << REFSWITCH_POL_OFF) & REFSWITCH_POL_MSK) |  \
    (((lor_missclk_pol) << LOR_MISSCLK_POL_OFF) & LOR_MISSCLK_POL_MSK) |  \
    (((lor_freq_pol) << LOR_FREQ_POL_OFF) & LOR_FREQ_POL_MSK) |  \
    (((lor_amp_pol) << LOR_AMP_POL_OFF) & LOR_AMP_POL_MSK))
// Register R19 [0x13] -- INT_FLAG0

enum int_flag0_fields_t {
    LOS_FDET_XO_INTR_OFF = 0x4,
    LOS_FDET_XO_INTR_MSK = 0x10,
    LOL_PLL2_INTR_OFF = 0x3,
    LOL_PLL2_INTR_MSK = 0x8,
    LOL_PLL1_INTR_OFF = 0x2,
    LOL_PLL1_INTR_MSK = 0x4,
    LOS_XO_INTR_OFF = 0x0,
    LOS_XO_INTR_MSK = 0x1,
};
#define MAKE_LMK05318_INT_FLAG0(los_fdet_xo_intr, lol_pll2_intr, lol_pll1_intr, los_xo_intr) MAKE_LMK05318_REG_WR(INT_FLAG0, \
    (((los_fdet_xo_intr) << LOS_FDET_XO_INTR_OFF) & LOS_FDET_XO_INTR_MSK) |  \
    (((lol_pll2_intr) << LOL_PLL2_INTR_OFF) & LOL_PLL2_INTR_MSK) |  \
    (((lol_pll1_intr) << LOL_PLL1_INTR_OFF) & LOL_PLL1_INTR_MSK) |  \
    (((los_xo_intr) << LOS_XO_INTR_OFF) & LOS_XO_INTR_MSK))
// Register R20 [0x14] -- INT_FLAG1

enum int_flag1_fields_t {
    LOPL_DPLL_INTR_OFF = 0x7,
    LOPL_DPLL_INTR_MSK = 0x80,
    LOFL_DPLL_INTR_OFF = 0x6,
    LOFL_DPLL_INTR_MSK = 0x40,
    HIST_INTR_OFF = 0x5,
    HIST_INTR_MSK = 0x20,
    HLDOVR_INTR_OFF = 0x4,
    HLDOVR_INTR_MSK = 0x10,
    REFSWITCH_INTR_OFF = 0x3,
    REFSWITCH_INTR_MSK = 0x8,
    LOR_MISSCLK_INTR_OFF = 0x2,
    LOR_MISSCLK_INTR_MSK = 0x4,
    LOR_FREQ_INTR_OFF = 0x1,
    LOR_FREQ_INTR_MSK = 0x2,
    LOR_AMP_INTR_OFF = 0x0,
    LOR_AMP_INTR_MSK = 0x1,
};
#define MAKE_LMK05318_INT_FLAG1(lopl_dpll_intr, lofl_dpll_intr, hist_intr, hldovr_intr, refswitch_intr, lor_missclk_intr, lor_freq_intr, lor_amp_intr) MAKE_LMK05318_REG_WR(INT_FLAG1, \
    (((lopl_dpll_intr) << LOPL_DPLL_INTR_OFF) & LOPL_DPLL_INTR_MSK) |  \
    (((lofl_dpll_intr) << LOFL_DPLL_INTR_OFF) & LOFL_DPLL_INTR_MSK) |  \
    (((hist_intr) << HIST_INTR_OFF) & HIST_INTR_MSK) |  \
    (((hldovr_intr) << HLDOVR_INTR_OFF) & HLDOVR_INTR_MSK) |  \
    (((refswitch_intr) << REFSWITCH_INTR_OFF) & REFSWITCH_INTR_MSK) |  \
    (((lor_missclk_intr) << LOR_MISSCLK_INTR_OFF) & LOR_MISSCLK_INTR_MSK) |  \
    (((lor_freq_intr) << LOR_FREQ_INTR_OFF) & LOR_FREQ_INTR_MSK) |  \
    (((lor_amp_intr) << LOR_AMP_INTR_OFF) & LOR_AMP_INTR_MSK))
// Register R21 [0x15] -- INTCTL

enum intctl_fields_t {
    INT_AND_OR_OFF = 0x1,
    INT_AND_OR_MSK = 0x2,
    INT_EN_OFF = 0x0,
    INT_EN_MSK = 0x1,
};
#define MAKE_LMK05318_INTCTL(int_and_or, int_en) MAKE_LMK05318_REG_WR(INTCTL, \
    (((int_and_or) << INT_AND_OR_OFF) & INT_AND_OR_MSK) |  \
    (((int_en) << INT_EN_OFF) & INT_EN_MSK))
// Register R22 [0x16] -- STAT_POL

enum stat_pol_fields_t {
    STAT1_POL_OFF = 0x1,
    STAT1_POL_MSK = 0x2,
    STAT0_POL_OFF = 0x0,
    STAT0_POL_MSK = 0x1,
};
#define MAKE_LMK05318_STAT_POL(stat1_pol, stat0_pol) MAKE_LMK05318_REG_WR(STAT_POL, \
    (((stat1_pol) << STAT1_POL_OFF) & STAT1_POL_MSK) |  \
    (((stat0_pol) << STAT0_POL_OFF) & STAT0_POL_MSK))
// Register R23 [0x17] -- MUTELVL1
enum ch3_mute_lvl_options {
    CH3_MUTE_LVL_BYPASS = 0,
    CH3_MUTE_LVL_DIFF_VOCM_P_BY_N_LOW = 1,
    CH3_MUTE_LVL_DIFF_HIGH_P_LOW_N_BY = 2,
    CH3_MUTE_LVL_DIFF_LOW_P_LOW_N_LOW = 3,
};
enum ch2_mute_lvl_options {
    CH2_MUTE_LVL_BYPASS = 0,
    CH2_MUTE_LVL_DIFF_VOCM_P_BY_N_LOW = 1,
    CH2_MUTE_LVL_DIFF_HIGH_P_LOW_N_BY = 2,
    CH2_MUTE_LVL_DIFF_LOW_P_LOW_N_LOW = 3,
};
enum ch1_mute_lvl_options {
    CH1_MUTE_LVL_BYPASS = 0,
    CH1_MUTE_LVL_DIFF_VOCM_P_BY_N_LOW = 1,
    CH1_MUTE_LVL_DIFF_HIGH_P_LOW_N_BY = 2,
    CH1_MUTE_LVL_DIFF_LOW_P_LOW_N_LOW = 3,
};
enum ch0_mute_lvl_options {
    CH0_MUTE_LVL_BYPASS = 0,
    CH0_MUTE_LVL_DIFF_VOCM_P_BY_N_LOW = 1,
    CH0_MUTE_LVL_DIFF_HIGH_P_LOW_N_BY = 2,
    CH0_MUTE_LVL_DIFF_LOW_P_LOW_N_LOW = 3,
};

enum mutelvl1_fields_t {
    CH3_MUTE_LVL_OFF = 0x6,
    CH3_MUTE_LVL_MSK = 0xc0,
    CH2_MUTE_LVL_OFF = 0x4,
    CH2_MUTE_LVL_MSK = 0x30,
    CH1_MUTE_LVL_OFF = 0x2,
    CH1_MUTE_LVL_MSK = 0xc,
    CH0_MUTE_LVL_OFF = 0x0,
    CH0_MUTE_LVL_MSK = 0x3,
};
#define MAKE_LMK05318_MUTELVL1(ch3_mute_lvl, ch2_mute_lvl, ch1_mute_lvl, ch0_mute_lvl) MAKE_LMK05318_REG_WR(MUTELVL1, \
    (((ch3_mute_lvl) << CH3_MUTE_LVL_OFF) & CH3_MUTE_LVL_MSK) |  \
    (((ch2_mute_lvl) << CH2_MUTE_LVL_OFF) & CH2_MUTE_LVL_MSK) |  \
    (((ch1_mute_lvl) << CH1_MUTE_LVL_OFF) & CH1_MUTE_LVL_MSK) |  \
    (((ch0_mute_lvl) << CH0_MUTE_LVL_OFF) & CH0_MUTE_LVL_MSK))
// Register R24 [0x18] -- MUTELVL2
enum ch7_mute_lvl_options {
    CH7_MUTE_LVL_BYPASS = 0,
    CH7_MUTE_LVL_DIFF_VOCM_P_BY_N_LOW = 1,
    CH7_MUTE_LVL_DIFF_HIGH_P_LOW_N_BY = 2,
    CH7_MUTE_LVL_DIFF_LOW_P_LOW_N_LOW = 3,
};
enum ch6_mute_lvl_options {
    CH6_MUTE_LVL_BYPASS = 0,
    CH6_MUTE_LVL_DIFF_VOCM_P_BY_N_LOW = 1,
    CH6_MUTE_LVL_DIFF_HIGH_P_LOW_N_BY = 2,
    CH6_MUTE_LVL_DIFF_LOW_P_LOW_N_LOW = 3,
};
enum ch5_mute_lvl_options {
    CH5_MUTE_LVL_BYPASS = 0,
    CH5_MUTE_LVL_DIFF_VOCM_P_BY_N_LOW = 1,
    CH5_MUTE_LVL_DIFF_HIGH_P_LOW_N_BY = 2,
    CH5_MUTE_LVL_DIFF_LOW_P_LOW_N_LOW = 3,
};
enum ch4_mute_lvl_options {
    CH4_MUTE_LVL_BYPASS = 0,
    CH4_MUTE_LVL_DIFF_VOCM_P_BY_N_LOW = 1,
    CH4_MUTE_LVL_DIFF_HIGH_P_LOW_N_BY = 2,
    CH4_MUTE_LVL_DIFF_LOW_P_LOW_N_LOW = 3,
};

enum mutelvl2_fields_t {
    CH7_MUTE_LVL_OFF = 0x6,
    CH7_MUTE_LVL_MSK = 0xc0,
    CH6_MUTE_LVL_OFF = 0x4,
    CH6_MUTE_LVL_MSK = 0x30,
    CH5_MUTE_LVL_OFF = 0x2,
    CH5_MUTE_LVL_MSK = 0xc,
    CH4_MUTE_LVL_OFF = 0x0,
    CH4_MUTE_LVL_MSK = 0x3,
};
#define MAKE_LMK05318_MUTELVL2(ch7_mute_lvl, ch6_mute_lvl, ch5_mute_lvl, ch4_mute_lvl) MAKE_LMK05318_REG_WR(MUTELVL2, \
    (((ch7_mute_lvl) << CH7_MUTE_LVL_OFF) & CH7_MUTE_LVL_MSK) |  \
    (((ch6_mute_lvl) << CH6_MUTE_LVL_OFF) & CH6_MUTE_LVL_MSK) |  \
    (((ch5_mute_lvl) << CH5_MUTE_LVL_OFF) & CH5_MUTE_LVL_MSK) |  \
    (((ch4_mute_lvl) << CH4_MUTE_LVL_OFF) & CH4_MUTE_LVL_MSK))
// Register R25 [0x19] -- OUT_MUTE

enum out_mute_fields_t {
    CH7_MUTE_OFF = 0x7,
    CH7_MUTE_MSK = 0x80,
    CH6_MUTE_OFF = 0x6,
    CH6_MUTE_MSK = 0x40,
    CH5_MUTE_OFF = 0x5,
    CH5_MUTE_MSK = 0x20,
    CH4_MUTE_OFF = 0x4,
    CH4_MUTE_MSK = 0x10,
    CH3_MUTE_OFF = 0x3,
    CH3_MUTE_MSK = 0x8,
    CH2_MUTE_OFF = 0x2,
    CH2_MUTE_MSK = 0x4,
    CH1_MUTE_OFF = 0x1,
    CH1_MUTE_MSK = 0x2,
    CH0_MUTE_OFF = 0x0,
    CH0_MUTE_MSK = 0x1,
};
#define MAKE_LMK05318_OUT_MUTE(ch7_mute, ch6_mute, ch5_mute, ch4_mute, ch3_mute, ch2_mute, ch1_mute, ch0_mute) MAKE_LMK05318_REG_WR(OUT_MUTE, \
    (((ch7_mute) << CH7_MUTE_OFF) & CH7_MUTE_MSK) |  \
    (((ch6_mute) << CH6_MUTE_OFF) & CH6_MUTE_MSK) |  \
    (((ch5_mute) << CH5_MUTE_OFF) & CH5_MUTE_MSK) |  \
    (((ch4_mute) << CH4_MUTE_OFF) & CH4_MUTE_MSK) |  \
    (((ch3_mute) << CH3_MUTE_OFF) & CH3_MUTE_MSK) |  \
    (((ch2_mute) << CH2_MUTE_OFF) & CH2_MUTE_MSK) |  \
    (((ch1_mute) << CH1_MUTE_OFF) & CH1_MUTE_MSK) |  \
    (((ch0_mute) << CH0_MUTE_OFF) & CH0_MUTE_MSK))
// Register R29 [0x1d] -- DPLL_MUTE

enum dpll_mute_fields_t {
    MUTE_APLL2_LOCK_OFF = 0x4,
    MUTE_APLL2_LOCK_MSK = 0x10,
    MUTE_DPLL_PHLOCK_OFF = 0x2,
    MUTE_DPLL_PHLOCK_MSK = 0x4,
    MUTE_DPLL_FLLOCK_OFF = 0x1,
    MUTE_DPLL_FLLOCK_MSK = 0x2,
    MUTE_APLL1_LOCK_OFF = 0x0,
    MUTE_APLL1_LOCK_MSK = 0x1,
};
#define MAKE_LMK05318_DPLL_MUTE(mute_apll2_lock, mute_dpll_phlock, mute_dpll_fllock, mute_apll1_lock) MAKE_LMK05318_REG_WR(DPLL_MUTE, \
    (((mute_apll2_lock) << MUTE_APLL2_LOCK_OFF) & MUTE_APLL2_LOCK_MSK) |  \
    (((mute_dpll_phlock) << MUTE_DPLL_PHLOCK_OFF) & MUTE_DPLL_PHLOCK_MSK) |  \
    (((mute_dpll_fllock) << MUTE_DPLL_FLLOCK_OFF) & MUTE_DPLL_FLLOCK_MSK) |  \
    (((mute_apll1_lock) << MUTE_APLL1_LOCK_OFF) & MUTE_APLL1_LOCK_MSK))
// Register R36 [0x24] -- GPIO_OUT

enum gpio_out_fields_t {
    GPIO_STAT1_OUT_OFF = 0x1,
    GPIO_STAT1_OUT_MSK = 0x2,
    GPIO_STAT0_OUT_OFF = 0x0,
    GPIO_STAT0_OUT_MSK = 0x1,
};
#define MAKE_LMK05318_GPIO_OUT(gpio_stat1_out, gpio_stat0_out) MAKE_LMK05318_REG_WR(GPIO_OUT, \
    (((gpio_stat1_out) << GPIO_STAT1_OUT_OFF) & GPIO_STAT1_OUT_MSK) |  \
    (((gpio_stat0_out) << GPIO_STAT0_OUT_OFF) & GPIO_STAT0_OUT_MSK))
// Register R39 [0x27] -- SPARE_NVMBASE2_BY2

enum spare_nvmbase2_by2_fields_t {
    GPIO2_OUT_OFF = 0x1,
    GPIO2_OUT_MSK = 0x2,
    APLL1_DEN_MODE_OFF = 0x0,
    APLL1_DEN_MODE_MSK = 0x1,
};
#define MAKE_LMK05318_SPARE_NVMBASE2_BY2(gpio2_out, apll1_den_mode) MAKE_LMK05318_REG_WR(SPARE_NVMBASE2_BY2, \
    (((gpio2_out) << GPIO2_OUT_OFF) & GPIO2_OUT_MSK) |  \
    (((apll1_den_mode) << APLL1_DEN_MODE_OFF) & APLL1_DEN_MODE_MSK))
// Register R40 [0x28] -- SPARE_NVMBASE2_BY1
enum secref_dc_mode_options {
    SECREF_DC_MODE_AC_COUPLED_INT = 0,
    SECREF_DC_MODE_DC_COUPLED_INT = 1,
};
enum priref_dc_mode_options {
    PRIREF_DC_MODE_AC_COUPLED_INT = 0,
    PRIREF_DC_MODE_DC_COUPLED_INT = 1,
};

enum spare_nvmbase2_by1_fields_t {
    SECREF_DC_MODE_OFF = 0x3,
    SECREF_DC_MODE_MSK = 0x8,
    PRIREF_DC_MODE_OFF = 0x2,
    PRIREF_DC_MODE_MSK = 0x4,
    APLL2_DEN_MODE_OFF = 0x0,
    APLL2_DEN_MODE_MSK = 0x1,
};
#define MAKE_LMK05318_SPARE_NVMBASE2_BY1(secref_dc_mode, priref_dc_mode, apll2_den_mode) MAKE_LMK05318_REG_WR(SPARE_NVMBASE2_BY1, \
    (((secref_dc_mode) << SECREF_DC_MODE_OFF) & SECREF_DC_MODE_MSK) |  \
    (((priref_dc_mode) << PRIREF_DC_MODE_OFF) & PRIREF_DC_MODE_MSK) |  \
    (((apll2_den_mode) << APLL2_DEN_MODE_OFF) & APLL2_DEN_MODE_MSK))
// Register R42 [0x2a] -- XO_CLKCTL1

enum xo_clkctl1_fields_t {
    OSCIN_DBLR_EN_OFF = 0x4,
    OSCIN_DBLR_EN_MSK = 0x10,
    XO_FDET_BYP_OFF = 0x3,
    XO_FDET_BYP_MSK = 0x8,
    XO_DETECT_BYP_OFF = 0x2,
    XO_DETECT_BYP_MSK = 0x4,
    XO_BUFSEL_OFF = 0x0,
    XO_BUFSEL_MSK = 0x1,
};
#define MAKE_LMK05318_XO_CLKCTL1(oscin_dblr_en, xo_fdet_byp, xo_detect_byp, xo_bufsel) MAKE_LMK05318_REG_WR(XO_CLKCTL1, \
    (((oscin_dblr_en) << OSCIN_DBLR_EN_OFF) & OSCIN_DBLR_EN_MSK) |  \
    (((xo_fdet_byp) << XO_FDET_BYP_OFF) & XO_FDET_BYP_MSK) |  \
    (((xo_detect_byp) << XO_DETECT_BYP_OFF) & XO_DETECT_BYP_MSK) |  \
    (((xo_bufsel) << XO_BUFSEL_OFF) & XO_BUFSEL_MSK))
// Register R43 [0x2b] -- XO_CLKCTL2
enum xo_type_options {
    XO_TYPE_DC_DIFF_EXT = 0,
    XO_TYPE_AC_DIFF_EXT = 1,
    XO_TYPE_AC_DIFF_INT_100 = 3,
    XO_TYPE_HCSL_INT_50 = 4,
    XO_TYPE_CMOS = 8,
    XO_TYPE_SE_INT_50 = 12,
};

enum xo_clkctl2_fields_t {
    XO_CLKCTL2_RESERVED7_OFF = 0x7,
    XO_CLKCTL2_RESERVED7_MSK = 0x80,
    XO_TYPE_OFF = 0x3,
    XO_TYPE_MSK = 0x78,
    XO_CLKCTL2_RESERVED0_OFF = 0x0,
    XO_CLKCTL2_RESERVED0_MSK = 0x7,
};
#define MAKE_LMK05318_XO_CLKCTL2(xo_clkctl2_reserved7, xo_type, xo_clkctl2_reserved0) MAKE_LMK05318_REG_WR(XO_CLKCTL2, \
    (((xo_clkctl2_reserved7) << XO_CLKCTL2_RESERVED7_OFF) & XO_CLKCTL2_RESERVED7_MSK) |  \
    (((xo_type) << XO_TYPE_OFF) & XO_TYPE_MSK) |  \
    (((xo_clkctl2_reserved0) << XO_CLKCTL2_RESERVED0_OFF) & XO_CLKCTL2_RESERVED0_MSK))
// Register R44 [0x2c] -- XO_CONFIG

enum xo_config_fields_t {
    OSCIN_RDIV_OFF = 0x0,
    OSCIN_RDIV_MSK = 0x1f,
};
#define MAKE_LMK05318_XO_CONFIG(oscin_rdiv) MAKE_LMK05318_REG_WR(XO_CONFIG, \
    (((oscin_rdiv) << OSCIN_RDIV_OFF) & OSCIN_RDIV_MSK))
// Register R45 [0x2d] -- REF_CLKCTL1
enum secref_buf_mode_options {
    SECREF_BUF_MODE_AC_HYST50_DC_EN = 0,
    SECREF_BUF_MODE_AC_HYST200_DC_DIS = 1,
};
enum priref_buf_mode_options {
    PRIREF_BUF_MODE_AC_HYST50_DC_EN = 0,
    PRIREF_BUF_MODE_AC_HYST200_DC_DIS = 1,
};

enum ref_clkctl1_fields_t {
    SECREF_CMOS_SLEW_OFF = 0x3,
    SECREF_CMOS_SLEW_MSK = 0x8,
    PRIREF_CMOS_SLEW_OFF = 0x2,
    PRIREF_CMOS_SLEW_MSK = 0x4,
    SECREF_BUF_MODE_OFF = 0x1,
    SECREF_BUF_MODE_MSK = 0x2,
    PRIREF_BUF_MODE_OFF = 0x0,
    PRIREF_BUF_MODE_MSK = 0x1,
};
#define MAKE_LMK05318_REF_CLKCTL1(secref_cmos_slew, priref_cmos_slew, secref_buf_mode, priref_buf_mode) MAKE_LMK05318_REG_WR(REF_CLKCTL1, \
    (((secref_cmos_slew) << SECREF_CMOS_SLEW_OFF) & SECREF_CMOS_SLEW_MSK) |  \
    (((priref_cmos_slew) << PRIREF_CMOS_SLEW_OFF) & PRIREF_CMOS_SLEW_MSK) |  \
    (((secref_buf_mode) << SECREF_BUF_MODE_OFF) & SECREF_BUF_MODE_MSK) |  \
    (((priref_buf_mode) << PRIREF_BUF_MODE_OFF) & PRIREF_BUF_MODE_MSK))
// Register R46 [0x2e] -- REF_CLKCTL2
enum secref_type_options {
    SECREF_TYPE_DIFF_NOTERM = 1,
    SECREF_TYPE_DIFF_100 = 3,
    SECREF_TYPE_DIFF_50 = 5,
    SECREF_TYPE_SE_NOTERM = 8,
    SECREF_TYPE_SE_50 = 12,
};
enum priref_type_options {
    PRIREF_TYPE_DIFF_NOTERM = 1,
    PRIREF_TYPE_DIFF_100 = 3,
    PRIREF_TYPE_DIFF_50 = 5,
    PRIREF_TYPE_SE_NOTERM = 8,
    PRIREF_TYPE_SE_50 = 12,
};

enum ref_clkctl2_fields_t {
    SECREF_TYPE_OFF = 0x4,
    SECREF_TYPE_MSK = 0xf0,
    PRIREF_TYPE_OFF = 0x0,
    PRIREF_TYPE_MSK = 0xf,
};
#define MAKE_LMK05318_REF_CLKCTL2(secref_type, priref_type) MAKE_LMK05318_REG_WR(REF_CLKCTL2, \
    (((secref_type) << SECREF_TYPE_OFF) & SECREF_TYPE_MSK) |  \
    (((priref_type) << PRIREF_TYPE_OFF) & PRIREF_TYPE_MSK))
// Register R47 [0x2f] -- PLL_CLK_CFG

enum pll_clk_cfg_fields_t {
    PLL2_RCLK_SEL_OFF = 0x7,
    PLL2_RCLK_SEL_MSK = 0x80,
    PLL1_VCO_TO_CNTRS_EN_OFF = 0x0,
    PLL1_VCO_TO_CNTRS_EN_MSK = 0x7,
};
#define MAKE_LMK05318_PLL_CLK_CFG(pll2_rclk_sel, pll1_vco_to_cntrs_en) MAKE_LMK05318_REG_WR(PLL_CLK_CFG, \
    (((pll2_rclk_sel) << PLL2_RCLK_SEL_OFF) & PLL2_RCLK_SEL_MSK) |  \
    (((pll1_vco_to_cntrs_en) << PLL1_VCO_TO_CNTRS_EN_OFF) & PLL1_VCO_TO_CNTRS_EN_MSK))
// Register R48 [0x30] -- STAT0_SEL

enum stat0_sel_fields_t {
    STAT0_SEL_OFF = 0x0,
    STAT0_SEL_MSK = 0x7f,
};
#define MAKE_LMK05318_STAT0_SEL(stat0_sel) MAKE_LMK05318_REG_WR(STAT0_SEL, \
    (((stat0_sel) << STAT0_SEL_OFF) & STAT0_SEL_MSK))
// Register R49 [0x31] -- STAT1_SEL

enum stat1_sel_fields_t {
    STAT1_SEL_OFF = 0x0,
    STAT1_SEL_MSK = 0x7f,
};
#define MAKE_LMK05318_STAT1_SEL(stat1_sel) MAKE_LMK05318_REG_WR(STAT1_SEL, \
    (((stat1_sel) << STAT1_SEL_OFF) & STAT1_SEL_MSK))
// Register R50 [0x32] -- PWDN

enum pwdn_fields_t {
    GPIO_FDEV_EN_OFF = 0x7,
    GPIO_FDEV_EN_MSK = 0x80,
    CH7_PD_OFF = 0x5,
    CH7_PD_MSK = 0x20,
    CH6_PD_OFF = 0x4,
    CH6_PD_MSK = 0x10,
    CH5_PD_OFF = 0x3,
    CH5_PD_MSK = 0x8,
    CH4_PD_OFF = 0x2,
    CH4_PD_MSK = 0x4,
    CH2_3_PD_OFF = 0x1,
    CH2_3_PD_MSK = 0x2,
    CH0_1_PD_OFF = 0x0,
    CH0_1_PD_MSK = 0x1,
};
#define MAKE_LMK05318_PWDN(gpio_fdev_en, ch7_pd, ch6_pd, ch5_pd, ch4_pd, ch2_3_pd, ch0_1_pd) MAKE_LMK05318_REG_WR(PWDN, \
    (((gpio_fdev_en) << GPIO_FDEV_EN_OFF) & GPIO_FDEV_EN_MSK) |  \
    (((ch7_pd) << CH7_PD_OFF) & CH7_PD_MSK) |  \
    (((ch6_pd) << CH6_PD_OFF) & CH6_PD_MSK) |  \
    (((ch5_pd) << CH5_PD_OFF) & CH5_PD_MSK) |  \
    (((ch4_pd) << CH4_PD_OFF) & CH4_PD_MSK) |  \
    (((ch2_3_pd) << CH2_3_PD_OFF) & CH2_3_PD_MSK) |  \
    (((ch0_1_pd) << CH0_1_PD_OFF) & CH0_1_PD_MSK))
// Register R51 [0x33] -- OUTCTL_0
enum ch0_1_mux_options {
    CH0_1_MUX_APLL1_P1 = 0,
    CH0_1_MUX_APLL1_P1_INV = 1,
    CH0_1_MUX_APLL2_P1 = 2,
    CH0_1_MUX_APLL2_P2 = 3,
};
enum out0_fmt_options {
    OUT0_FMT_DISABLED = 0,
    OUT0_FMT_AC_LVDS = 16,
    OUT0_FMT_AC_CML = 20,
    OUT0_FMT_AC_LVPECL = 24,
    OUT0_FMT_HCSL_EXT_50 = 44,
    OUT0_FMT_HCSL_INT_50 = 45,
    OUT0_FMT_LVCMOS_HIZ_HIZ = 48,
    OUT0_FMT_LVCMOS_HIZ_N = 50,
    OUT0_FMT_LVCMOS_HIZ_P = 51,
    OUT0_FMT_LVCMOS_LOW_LOW = 53,
    OUT0_FMT_LVCMOS_N_HIZ = 56,
    OUT0_FMT_LVCMOS_N_N = 58,
    OUT0_FMT_LVCMOS_N_P = 59,
    OUT0_FMT_LVCMOS_P_HIZ = 60,
    OUT0_FMT_LVCMOS_P_N = 62,
    OUT0_FMT_LVCMOS_P_P = 63,
};

enum outctl_0_fields_t {
    CH0_1_MUX_OFF = 0x6,
    CH0_1_MUX_MSK = 0xc0,
    OUT0_FMT_OFF = 0x0,
    OUT0_FMT_MSK = 0x3f,
};
#define MAKE_LMK05318_OUTCTL_0(ch0_1_mux, out0_fmt) MAKE_LMK05318_REG_WR(OUTCTL_0, \
    (((ch0_1_mux) << CH0_1_MUX_OFF) & CH0_1_MUX_MSK) |  \
    (((out0_fmt) << OUT0_FMT_OFF) & OUT0_FMT_MSK))
// Register R52 [0x34] -- OUTCTL_1
enum out1_fmt_options {
    OUT1_FMT_DISABLED = 0,
    OUT1_FMT_AC_LVDS = 16,
    OUT1_FMT_AC_CML = 20,
    OUT1_FMT_AC_LVPECL = 24,
    OUT1_FMT_HCSL_EXT_50 = 44,
    OUT1_FMT_HCSL_INT_50 = 45,
    OUT1_FMT_LVCMOS_HIZ_HIZ = 48,
    OUT1_FMT_LVCMOS_HIZ_N = 50,
    OUT1_FMT_LVCMOS_HIZ_P = 51,
    OUT1_FMT_LVCMOS_LOW_LOW = 53,
    OUT1_FMT_LVCMOS_N_HIZ = 56,
    OUT1_FMT_LVCMOS_N_N = 58,
    OUT1_FMT_LVCMOS_N_P = 59,
    OUT1_FMT_LVCMOS_P_HIZ = 60,
    OUT1_FMT_LVCMOS_P_N = 62,
    OUT1_FMT_LVCMOS_P_P = 63,
};

enum outctl_1_fields_t {
    OUT1_FMT_OFF = 0x0,
    OUT1_FMT_MSK = 0x3f,
};
#define MAKE_LMK05318_OUTCTL_1(out1_fmt) MAKE_LMK05318_REG_WR(OUTCTL_1, \
    (((out1_fmt) << OUT1_FMT_OFF) & OUT1_FMT_MSK))
// Register R53 [0x35] -- OUTDIV_0_1

enum outdiv_0_1_fields_t {
    OUT0_1_DIV_OFF = 0x0,
    OUT0_1_DIV_MSK = 0xff,
};
#define MAKE_LMK05318_OUTDIV_0_1(out0_1_div) MAKE_LMK05318_REG_WR(OUTDIV_0_1, \
    (((out0_1_div) << OUT0_1_DIV_OFF) & OUT0_1_DIV_MSK))
// Register R54 [0x36] -- OUTCTL_2
enum ch2_3_mux_options {
    CH2_3_MUX_APLL1_P1 = 0,
    CH2_3_MUX_APLL1_P1_INV = 1,
    CH2_3_MUX_APLL2_P1 = 2,
    CH2_3_MUX_APLL2_P2 = 3,
};
enum out2_fmt_options {
    OUT2_FMT_DISABLED = 0,
    OUT2_FMT_AC_LVDS = 16,
    OUT2_FMT_AC_CML = 20,
    OUT2_FMT_AC_LVPECL = 24,
    OUT2_FMT_HCSL_EXT_50 = 44,
    OUT2_FMT_HCSL_INT_50 = 45,
    OUT2_FMT_LVCMOS_HIZ_HIZ = 48,
    OUT2_FMT_LVCMOS_HIZ_N = 50,
    OUT2_FMT_LVCMOS_HIZ_P = 51,
    OUT2_FMT_LVCMOS_LOW_LOW = 53,
    OUT2_FMT_LVCMOS_N_HIZ = 56,
    OUT2_FMT_LVCMOS_N_N = 58,
    OUT2_FMT_LVCMOS_N_P = 59,
    OUT2_FMT_LVCMOS_P_HIZ = 60,
    OUT2_FMT_LVCMOS_P_N = 62,
    OUT2_FMT_LVCMOS_P_P = 63,
};

enum outctl_2_fields_t {
    CH2_3_MUX_OFF = 0x6,
    CH2_3_MUX_MSK = 0xc0,
    OUT2_FMT_OFF = 0x0,
    OUT2_FMT_MSK = 0x3f,
};
#define MAKE_LMK05318_OUTCTL_2(ch2_3_mux, out2_fmt) MAKE_LMK05318_REG_WR(OUTCTL_2, \
    (((ch2_3_mux) << CH2_3_MUX_OFF) & CH2_3_MUX_MSK) |  \
    (((out2_fmt) << OUT2_FMT_OFF) & OUT2_FMT_MSK))
// Register R55 [0x37] -- OUTCTL_3
enum out3_fmt_options {
    OUT3_FMT_DISABLED = 0,
    OUT3_FMT_AC_LVDS = 16,
    OUT3_FMT_AC_CML = 20,
    OUT3_FMT_AC_LVPECL = 24,
    OUT3_FMT_HCSL_EXT_50 = 44,
    OUT3_FMT_HCSL_INT_50 = 45,
    OUT3_FMT_LVCMOS_HIZ_HIZ = 48,
    OUT3_FMT_LVCMOS_HIZ_N = 50,
    OUT3_FMT_LVCMOS_HIZ_P = 51,
    OUT3_FMT_LVCMOS_LOW_LOW = 53,
    OUT3_FMT_LVCMOS_N_HIZ = 56,
    OUT3_FMT_LVCMOS_N_N = 58,
    OUT3_FMT_LVCMOS_N_P = 59,
    OUT3_FMT_LVCMOS_P_HIZ = 60,
    OUT3_FMT_LVCMOS_P_N = 62,
    OUT3_FMT_LVCMOS_P_P = 63,
};

enum outctl_3_fields_t {
    OUT3_FMT_OFF = 0x0,
    OUT3_FMT_MSK = 0x3f,
};
#define MAKE_LMK05318_OUTCTL_3(out3_fmt) MAKE_LMK05318_REG_WR(OUTCTL_3, \
    (((out3_fmt) << OUT3_FMT_OFF) & OUT3_FMT_MSK))
// Register R56 [0x38] -- OUTDIV_2_3

enum outdiv_2_3_fields_t {
    OUT2_3_DIV_OFF = 0x0,
    OUT2_3_DIV_MSK = 0xff,
};
#define MAKE_LMK05318_OUTDIV_2_3(out2_3_div) MAKE_LMK05318_REG_WR(OUTDIV_2_3, \
    (((out2_3_div) << OUT2_3_DIV_OFF) & OUT2_3_DIV_MSK))
// Register R57 [0x39] -- OUTCTL_4
enum ch4_mux_options {
    CH4_MUX_APLL1_P1 = 0,
    CH4_MUX_APLL1_P1_INV = 1,
    CH4_MUX_APLL2_P1 = 2,
    CH4_MUX_APLL2_P2 = 3,
};
enum out4_fmt_options {
    OUT4_FMT_DISABLED = 0,
    OUT4_FMT_AC_LVDS = 16,
    OUT4_FMT_AC_CML = 20,
    OUT4_FMT_AC_LVPECL = 24,
    OUT4_FMT_HCSL_EXT_50 = 44,
    OUT4_FMT_HCSL_INT_50 = 45,
    OUT4_FMT_LVCMOS_HIZ_HIZ = 48,
    OUT4_FMT_LVCMOS_HIZ_N = 50,
    OUT4_FMT_LVCMOS_HIZ_P = 51,
    OUT4_FMT_LVCMOS_LOW_LOW = 53,
    OUT4_FMT_LVCMOS_N_HIZ = 56,
    OUT4_FMT_LVCMOS_N_N = 58,
    OUT4_FMT_LVCMOS_N_P = 59,
    OUT4_FMT_LVCMOS_P_HIZ = 60,
    OUT4_FMT_LVCMOS_P_N = 62,
    OUT4_FMT_LVCMOS_P_P = 63,
};

enum outctl_4_fields_t {
    CH4_MUX_OFF = 0x6,
    CH4_MUX_MSK = 0xc0,
    OUT4_FMT_OFF = 0x0,
    OUT4_FMT_MSK = 0x3f,
};
#define MAKE_LMK05318_OUTCTL_4(ch4_mux, out4_fmt) MAKE_LMK05318_REG_WR(OUTCTL_4, \
    (((ch4_mux) << CH4_MUX_OFF) & CH4_MUX_MSK) |  \
    (((out4_fmt) << OUT4_FMT_OFF) & OUT4_FMT_MSK))
// Register R58 [0x3a] -- OUTDIV_4

enum outdiv_4_fields_t {
    OUT4_DIV_OFF = 0x0,
    OUT4_DIV_MSK = 0xff,
};
#define MAKE_LMK05318_OUTDIV_4(out4_div) MAKE_LMK05318_REG_WR(OUTDIV_4, \
    (((out4_div) << OUT4_DIV_OFF) & OUT4_DIV_MSK))
// Register R59 [0x3b] -- OUTCTL_5
enum ch5_mux_options {
    CH5_MUX_APLL1_P1 = 0,
    CH5_MUX_APLL1_P1_INV = 1,
    CH5_MUX_APLL2_P1 = 2,
    CH5_MUX_APLL2_P2 = 3,
};
enum out5_fmt_options {
    OUT5_FMT_DISABLED = 0,
    OUT5_FMT_AC_LVDS = 16,
    OUT5_FMT_AC_CML = 20,
    OUT5_FMT_AC_LVPECL = 24,
    OUT5_FMT_HCSL_EXT_50 = 44,
    OUT5_FMT_HCSL_INT_50 = 45,
    OUT5_FMT_LVCMOS_HIZ_HIZ = 48,
    OUT5_FMT_LVCMOS_HIZ_N = 50,
    OUT5_FMT_LVCMOS_HIZ_P = 51,
    OUT5_FMT_LVCMOS_LOW_LOW = 53,
    OUT5_FMT_LVCMOS_N_HIZ = 56,
    OUT5_FMT_LVCMOS_N_N = 58,
    OUT5_FMT_LVCMOS_N_P = 59,
    OUT5_FMT_LVCMOS_P_HIZ = 60,
    OUT5_FMT_LVCMOS_P_N = 62,
    OUT5_FMT_LVCMOS_P_P = 63,
};

enum outctl_5_fields_t {
    CH5_MUX_OFF = 0x6,
    CH5_MUX_MSK = 0xc0,
    OUT5_FMT_OFF = 0x0,
    OUT5_FMT_MSK = 0x3f,
};
#define MAKE_LMK05318_OUTCTL_5(ch5_mux, out5_fmt) MAKE_LMK05318_REG_WR(OUTCTL_5, \
    (((ch5_mux) << CH5_MUX_OFF) & CH5_MUX_MSK) |  \
    (((out5_fmt) << OUT5_FMT_OFF) & OUT5_FMT_MSK))
// Register R60 [0x3c] -- OUTDIV_5

enum outdiv_5_fields_t {
    OUT5_DIV_OFF = 0x0,
    OUT5_DIV_MSK = 0xff,
};
#define MAKE_LMK05318_OUTDIV_5(out5_div) MAKE_LMK05318_REG_WR(OUTDIV_5, \
    (((out5_div) << OUT5_DIV_OFF) & OUT5_DIV_MSK))
// Register R61 [0x3d] -- OUTCTL_6
enum ch6_mux_options {
    CH6_MUX_APLL1_P1 = 0,
    CH6_MUX_APLL1_P1_INV = 1,
    CH6_MUX_APLL2_P1 = 2,
    CH6_MUX_APLL2_P2 = 3,
};
enum out6_fmt_options {
    OUT6_FMT_DISABLED = 0,
    OUT6_FMT_AC_LVDS = 16,
    OUT6_FMT_AC_CML = 20,
    OUT6_FMT_AC_LVPECL = 24,
    OUT6_FMT_HCSL_EXT_50 = 44,
    OUT6_FMT_HCSL_INT_50 = 45,
    OUT6_FMT_LVCMOS_HIZ_HIZ = 48,
    OUT6_FMT_LVCMOS_HIZ_N = 50,
    OUT6_FMT_LVCMOS_HIZ_P = 51,
    OUT6_FMT_LVCMOS_LOW_LOW = 53,
    OUT6_FMT_LVCMOS_N_HIZ = 56,
    OUT6_FMT_LVCMOS_N_N = 58,
    OUT6_FMT_LVCMOS_N_P = 59,
    OUT6_FMT_LVCMOS_P_HIZ = 60,
    OUT6_FMT_LVCMOS_P_N = 62,
    OUT6_FMT_LVCMOS_P_P = 63,
};

enum outctl_6_fields_t {
    CH6_MUX_OFF = 0x6,
    CH6_MUX_MSK = 0xc0,
    OUT6_FMT_OFF = 0x0,
    OUT6_FMT_MSK = 0x3f,
};
#define MAKE_LMK05318_OUTCTL_6(ch6_mux, out6_fmt) MAKE_LMK05318_REG_WR(OUTCTL_6, \
    (((ch6_mux) << CH6_MUX_OFF) & CH6_MUX_MSK) |  \
    (((out6_fmt) << OUT6_FMT_OFF) & OUT6_FMT_MSK))
// Register R62 [0x3e] -- OUTDIV_6

enum outdiv_6_fields_t {
    OUT6_DIV_OFF = 0x0,
    OUT6_DIV_MSK = 0xff,
};
#define MAKE_LMK05318_OUTDIV_6(out6_div) MAKE_LMK05318_REG_WR(OUTDIV_6, \
    (((out6_div) << OUT6_DIV_OFF) & OUT6_DIV_MSK))
// Register R63 [0x3f] -- OUTCTL_7
enum ch7_mux_options {
    CH7_MUX_APLL1_P1 = 0,
    CH7_MUX_APLL1_P1_INV = 1,
    CH7_MUX_APLL2_P1 = 2,
    CH7_MUX_APLL2_P2 = 3,
};
enum out7_fmt_options {
    OUT7_FMT_DISABLED = 0,
    OUT7_FMT_AC_LVDS = 16,
    OUT7_FMT_AC_CML = 20,
    OUT7_FMT_AC_LVPECL = 24,
    OUT7_FMT_HCSL_EXT_50 = 44,
    OUT7_FMT_HCSL_INT_50 = 45,
    OUT7_FMT_LVCMOS_HIZ_HIZ = 48,
    OUT7_FMT_LVCMOS_HIZ_N = 50,
    OUT7_FMT_LVCMOS_HIZ_P = 51,
    OUT7_FMT_LVCMOS_LOW_LOW = 53,
    OUT7_FMT_LVCMOS_N_HIZ = 56,
    OUT7_FMT_LVCMOS_N_N = 58,
    OUT7_FMT_LVCMOS_N_P = 59,
    OUT7_FMT_LVCMOS_P_HIZ = 60,
    OUT7_FMT_LVCMOS_P_N = 62,
    OUT7_FMT_LVCMOS_P_P = 63,
};

enum outctl_7_fields_t {
    CH7_MUX_OFF = 0x6,
    CH7_MUX_MSK = 0xc0,
    OUT7_FMT_OFF = 0x0,
    OUT7_FMT_MSK = 0x3f,
};
#define MAKE_LMK05318_OUTCTL_7(ch7_mux, out7_fmt) MAKE_LMK05318_REG_WR(OUTCTL_7, \
    (((ch7_mux) << CH7_MUX_OFF) & CH7_MUX_MSK) |  \
    (((out7_fmt) << OUT7_FMT_OFF) & OUT7_FMT_MSK))
// Register R64 [0x40] -- OUTDIV_7_STG2

enum outdiv_7_stg2_fields_t {
    OUT7_STG2_DIV_OFF = 0x0,
    OUT7_STG2_DIV_MSK = 0xffffff,
};
#define MAKE_LMK05318_OUTDIV_7_STG2_BY0(value) MAKE_LMK05318_REG_WR(OUTDIV_7_STG2_BY0, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_OUTDIV_7_STG2_BY1(value) MAKE_LMK05318_REG_WR(OUTDIV_7_STG2_BY1, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_OUTDIV_7_STG2_BY2(value) MAKE_LMK05318_REG_WR(OUTDIV_7_STG2_BY2, (((value) << 0) & 0xff))
// Register R67 [0x43] -- OUTDIV_7

enum outdiv_7_fields_t {
    OUT7_DIV_OFF = 0x0,
    OUT7_DIV_MSK = 0xff,
};
#define MAKE_LMK05318_OUTDIV_7(out7_div) MAKE_LMK05318_REG_WR(OUTDIV_7, \
    (((out7_div) << OUT7_DIV_OFF) & OUT7_DIV_MSK))
// Register R68 [0x44] -- PREDRIVER

enum predriver_fields_t {
    PREDRIVER_RESERVED_OFF = 0x4,
    PREDRIVER_RESERVED_MSK = 0xf0,
    PLL1_CP_BAW_OFF = 0x0,
    PLL1_CP_BAW_MSK = 0xf,
};
#define MAKE_LMK05318_PREDRIVER(predriver_reserved, pll1_cp_baw) MAKE_LMK05318_REG_WR(PREDRIVER, \
    (((predriver_reserved) << PREDRIVER_RESERVED_OFF) & PREDRIVER_RESERVED_MSK) |  \
    (((pll1_cp_baw) << PLL1_CP_BAW_OFF) & PLL1_CP_BAW_MSK))
// Register R70 [0x46] -- OUTSYNCCTL

enum outsyncctl_fields_t {
    PLL2_P2_SYNC_EN_OFF = 0x2,
    PLL2_P2_SYNC_EN_MSK = 0x4,
    PLL2_P1_SYNC_EN_OFF = 0x1,
    PLL2_P1_SYNC_EN_MSK = 0x2,
    PLL1_P1_SYNC_EN_OFF = 0x0,
    PLL1_P1_SYNC_EN_MSK = 0x1,
};
#define MAKE_LMK05318_OUTSYNCCTL(pll2_p2_sync_en, pll2_p1_sync_en, pll1_p1_sync_en) MAKE_LMK05318_REG_WR(OUTSYNCCTL, \
    (((pll2_p2_sync_en) << PLL2_P2_SYNC_EN_OFF) & PLL2_P2_SYNC_EN_MSK) |  \
    (((pll2_p1_sync_en) << PLL2_P1_SYNC_EN_OFF) & PLL2_P1_SYNC_EN_MSK) |  \
    (((pll1_p1_sync_en) << PLL1_P1_SYNC_EN_OFF) & PLL1_P1_SYNC_EN_MSK))
// Register R71 [0x47] -- OUTSYNCEN

enum outsyncen_fields_t {
    CH7_SYNC_EN_OFF = 0x5,
    CH7_SYNC_EN_MSK = 0x20,
    CH6_SYNC_EN_OFF = 0x4,
    CH6_SYNC_EN_MSK = 0x10,
    CH5_SYNC_EN_OFF = 0x3,
    CH5_SYNC_EN_MSK = 0x8,
    CH4_SYNC_EN_OFF = 0x2,
    CH4_SYNC_EN_MSK = 0x4,
    CH2_3_SYNC_EN_OFF = 0x1,
    CH2_3_SYNC_EN_MSK = 0x2,
    CH0_1_SYNC_EN_OFF = 0x0,
    CH0_1_SYNC_EN_MSK = 0x1,
};
#define MAKE_LMK05318_OUTSYNCEN(ch7_sync_en, ch6_sync_en, ch5_sync_en, ch4_sync_en, ch2_3_sync_en, ch0_1_sync_en) MAKE_LMK05318_REG_WR(OUTSYNCEN, \
    (((ch7_sync_en) << CH7_SYNC_EN_OFF) & CH7_SYNC_EN_MSK) |  \
    (((ch6_sync_en) << CH6_SYNC_EN_OFF) & CH6_SYNC_EN_MSK) |  \
    (((ch5_sync_en) << CH5_SYNC_EN_OFF) & CH5_SYNC_EN_MSK) |  \
    (((ch4_sync_en) << CH4_SYNC_EN_OFF) & CH4_SYNC_EN_MSK) |  \
    (((ch2_3_sync_en) << CH2_3_SYNC_EN_OFF) & CH2_3_SYNC_EN_MSK) |  \
    (((ch0_1_sync_en) << CH0_1_SYNC_EN_OFF) & CH0_1_SYNC_EN_MSK))
// Register R74 [0x4a] -- PLL1_CTRL0

enum pll1_ctrl0_fields_t {
    PLL1_PDN_OFF = 0x0,
    PLL1_PDN_MSK = 0x1,
};
#define MAKE_LMK05318_PLL1_CTRL0(pll1_pdn) MAKE_LMK05318_REG_WR(PLL1_CTRL0, \
    (((pll1_pdn) << PLL1_PDN_OFF) & PLL1_PDN_MSK))
// Register R79 [0x4f] -- PLL1_CALCTRL0

enum pll1_calctrl0_fields_t {
    BAW_LOCKDET_EN_OFF = 0x4,
    BAW_LOCKDET_EN_MSK = 0x10,
    PLL1_CLSDWAIT_OFF = 0x2,
    PLL1_CLSDWAIT_MSK = 0xc,
    PLL1_VCOWAIT_OFF = 0x0,
    PLL1_VCOWAIT_MSK = 0x3,
};
#define MAKE_LMK05318_PLL1_CALCTRL0(baw_lockdet_en, pll1_clsdwait, pll1_vcowait) MAKE_LMK05318_REG_WR(PLL1_CALCTRL0, \
    (((baw_lockdet_en) << BAW_LOCKDET_EN_OFF) & BAW_LOCKDET_EN_MSK) |  \
    (((pll1_clsdwait) << PLL1_CLSDWAIT_OFF) & PLL1_CLSDWAIT_MSK) |  \
    (((pll1_vcowait) << PLL1_VCOWAIT_OFF) & PLL1_VCOWAIT_MSK))
// Register R80 [0x50] -- BAW_LOCKDET_PPM_MAX_BY1

enum baw_lockdet_ppm_max_by1_fields_t {
    BAW_LOCK_OFF = 0x7,
    BAW_LOCK_MSK = 0x80,
    BAW_LOCK_DET_1_OFF = 0x0,
    BAW_LOCK_DET_1_MSK = 0x7f,
};
#define MAKE_LMK05318_BAW_LOCKDET_PPM_MAX_BY1(baw_lock, baw_lock_det_1) MAKE_LMK05318_REG_WR(BAW_LOCKDET_PPM_MAX_BY1, \
    (((baw_lock) << BAW_LOCK_OFF) & BAW_LOCK_MSK) |  \
    (((baw_lock_det_1) << BAW_LOCK_DET_1_OFF) & BAW_LOCK_DET_1_MSK))
// Register R81 [0x51] -- BAW_LOCKDET_PPM_MAX_BY0

enum baw_lockdet_ppm_max_by0_fields_t {
    BAW_LOCK_DET_2_OFF = 0x0,
    BAW_LOCK_DET_2_MSK = 0xff,
};
#define MAKE_LMK05318_BAW_LOCKDET_PPM_MAX_BY0(baw_lock_det_2) MAKE_LMK05318_REG_WR(BAW_LOCKDET_PPM_MAX_BY0, \
    (((baw_lock_det_2) << BAW_LOCK_DET_2_OFF) & BAW_LOCK_DET_2_MSK))
// Register R82 [0x52] -- BAW_LOCKDET_CNTSTRT


#define MAKE_LMK05318_BAW_LOCKDET_CNTSTRT_BY0(value) MAKE_LMK05318_REG_WR(BAW_LOCKDET_CNTSTRT_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_BAW_LOCKDET_CNTSTRT_BY1(value) MAKE_LMK05318_REG_WR(BAW_LOCKDET_CNTSTRT_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_BAW_LOCKDET_CNTSTRT_BY2(value) MAKE_LMK05318_REG_WR(BAW_LOCKDET_CNTSTRT_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_BAW_LOCKDET_CNTSTRT_BY3(value) MAKE_LMK05318_REG_WR(BAW_LOCKDET_CNTSTRT_BY3, (((value) << 0) & 0xff))
// Register R86 [0x56] -- BAW_LOCKDET_VCO_CNTSTRT


#define MAKE_LMK05318_BAW_LOCKDET_VCO_CNTSTRT_BY0(value) MAKE_LMK05318_REG_WR(BAW_LOCKDET_VCO_CNTSTRT_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_BAW_LOCKDET_VCO_CNTSTRT_BY1(value) MAKE_LMK05318_REG_WR(BAW_LOCKDET_VCO_CNTSTRT_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_BAW_LOCKDET_VCO_CNTSTRT_BY2(value) MAKE_LMK05318_REG_WR(BAW_LOCKDET_VCO_CNTSTRT_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_BAW_LOCKDET_VCO_CNTSTRT_BY3(value) MAKE_LMK05318_REG_WR(BAW_LOCKDET_VCO_CNTSTRT_BY3, (((value) << 0) & 0xff))
// Register R90 [0x5a] -- BAW_UNLOCKDET_PPM_MAX


#define MAKE_LMK05318_BAW_UNLOCKDET_PPM_MAX_BY0(value) MAKE_LMK05318_REG_WR(BAW_UNLOCKDET_PPM_MAX_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_BAW_UNLOCKDET_PPM_MAX_BY1(value) MAKE_LMK05318_REG_WR(BAW_UNLOCKDET_PPM_MAX_BY1, (((value) << 0) & 0xff))
// Register R92 [0x5c] -- BAW_UNLOCKDET_CNTSTRT


#define MAKE_LMK05318_BAW_UNLOCKDET_CNTSTRT_BY0(value) MAKE_LMK05318_REG_WR(BAW_UNLOCKDET_CNTSTRT_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_BAW_UNLOCKDET_CNTSTRT_BY1(value) MAKE_LMK05318_REG_WR(BAW_UNLOCKDET_CNTSTRT_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_BAW_UNLOCKDET_CNTSTRT_BY2(value) MAKE_LMK05318_REG_WR(BAW_UNLOCKDET_CNTSTRT_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_BAW_UNLOCKDET_CNTSTRT_BY3(value) MAKE_LMK05318_REG_WR(BAW_UNLOCKDET_CNTSTRT_BY3, (((value) << 0) & 0xff))
// Register R96 [0x60] -- BAW_UNLOCKDET_VCO_CNTSTRT


#define MAKE_LMK05318_BAW_UNLOCKDET_VCO_CNTSTRT_BY0(value) MAKE_LMK05318_REG_WR(BAW_UNLOCKDET_VCO_CNTSTRT_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_BAW_UNLOCKDET_VCO_CNTSTRT_BY1(value) MAKE_LMK05318_REG_WR(BAW_UNLOCKDET_VCO_CNTSTRT_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_BAW_UNLOCKDET_VCO_CNTSTRT_BY2(value) MAKE_LMK05318_REG_WR(BAW_UNLOCKDET_VCO_CNTSTRT_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_BAW_UNLOCKDET_VCO_CNTSTRT_BY3(value) MAKE_LMK05318_REG_WR(BAW_UNLOCKDET_VCO_CNTSTRT_BY3, (((value) << 0) & 0xff))
// Register R100 [0x64] -- PLL2_CTRL0

enum pll2_ctrl0_fields_t {
    PLL2_RDIV_SEC_OFF = 0x3,
    PLL2_RDIV_SEC_MSK = 0xf8,
    PLL2_RDIV_PRE_OFF = 0x1,
    PLL2_RDIV_PRE_MSK = 0x6,
    PLL2_PDN_OFF = 0x0,
    PLL2_PDN_MSK = 0x1,
};
#define MAKE_LMK05318_PLL2_CTRL0(pll2_rdiv_sec, pll2_rdiv_pre, pll2_pdn) MAKE_LMK05318_REG_WR(PLL2_CTRL0, \
    (((pll2_rdiv_sec) << PLL2_RDIV_SEC_OFF) & PLL2_RDIV_SEC_MSK) |  \
    (((pll2_rdiv_pre) << PLL2_RDIV_PRE_OFF) & PLL2_RDIV_PRE_MSK) |  \
    (((pll2_pdn) << PLL2_PDN_OFF) & PLL2_PDN_MSK))
// Register R101 [0x65] -- PLL2_CTRL1

enum pll2_ctrl1_fields_t {
    PLL2_VM_BYP_OFF = 0x2,
    PLL2_VM_BYP_MSK = 0x4,
    PLL2_CP_OFF = 0x0,
    PLL2_CP_MSK = 0x3,
};
#define MAKE_LMK05318_PLL2_CTRL1(pll2_vm_byp, pll2_cp) MAKE_LMK05318_REG_WR(PLL2_CTRL1, \
    (((pll2_vm_byp) << PLL2_VM_BYP_OFF) & PLL2_VM_BYP_MSK) |  \
    (((pll2_cp) << PLL2_CP_OFF) & PLL2_CP_MSK))
// Register R102 [0x66] -- PLL2_CTRL2

enum pll2_ctrl2_fields_t {
    PLL2_P2_OFF = 0x4,
    PLL2_P2_MSK = 0x70,
    PLL2_P1_OFF = 0x0,
    PLL2_P1_MSK = 0x7,
};
#define MAKE_LMK05318_PLL2_CTRL2(pll2_p2, pll2_p1) MAKE_LMK05318_REG_WR(PLL2_CTRL2, \
    (((pll2_p2) << PLL2_P2_OFF) & PLL2_P2_MSK) |  \
    (((pll2_p1) << PLL2_P1_OFF) & PLL2_P1_MSK))
// Register R104 [0x68] -- PLL2_CTRL4

enum pll2_ctrl4_fields_t {
    PLL2_RBLEED_CP_OFF = 0x0,
    PLL2_RBLEED_CP_MSK = 0x3f,
};
#define MAKE_LMK05318_PLL2_CTRL4(pll2_rbleed_cp) MAKE_LMK05318_REG_WR(PLL2_CTRL4, \
    (((pll2_rbleed_cp) << PLL2_RBLEED_CP_OFF) & PLL2_RBLEED_CP_MSK))
// Register R105 [0x69] -- PLL2_CALCTRL0

enum pll2_calctrl0_fields_t {
    PLL2_CLSDWAIT_OFF = 0x2,
    PLL2_CLSDWAIT_MSK = 0xc,
    PLL2_VCOWAIT_OFF = 0x0,
    PLL2_VCOWAIT_MSK = 0x3,
};
#define MAKE_LMK05318_PLL2_CALCTRL0(pll2_clsdwait, pll2_vcowait) MAKE_LMK05318_REG_WR(PLL2_CALCTRL0, \
    (((pll2_clsdwait) << PLL2_CLSDWAIT_OFF) & PLL2_CLSDWAIT_MSK) |  \
    (((pll2_vcowait) << PLL2_VCOWAIT_OFF) & PLL2_VCOWAIT_MSK))
// Register R108 [0x6c] -- PLL1_NDIV


#define MAKE_LMK05318_PLL1_NDIV_BY0(value) MAKE_LMK05318_REG_WR(PLL1_NDIV_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_PLL1_NDIV_BY1(value) MAKE_LMK05318_REG_WR(PLL1_NDIV_BY1, (((value) << 0) & 0xff))
// Register R110 [0x6e] -- PLL1_NUM


#define MAKE_LMK05318_PLL1_NUM_BY0(value) MAKE_LMK05318_REG_WR(PLL1_NUM_BY0, (((value) >> 32) & 0xff))
#define MAKE_LMK05318_PLL1_NUM_BY1(value) MAKE_LMK05318_REG_WR(PLL1_NUM_BY1, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_PLL1_NUM_BY2(value) MAKE_LMK05318_REG_WR(PLL1_NUM_BY2, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_PLL1_NUM_BY3(value) MAKE_LMK05318_REG_WR(PLL1_NUM_BY3, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_PLL1_NUM_BY4(value) MAKE_LMK05318_REG_WR(PLL1_NUM_BY4, (((value) << 0) & 0xff))
// Register R115 [0x73] -- PLL1_MASHCTRL

enum pll1_mashctrl_fields_t {
    PLL1_DUAL_PH_EN_OFF = 0x7,
    PLL1_DUAL_PH_EN_MSK = 0x80,
    PLL1_MASHSEED1_OFF = 0x6,
    PLL1_MASHSEED1_MSK = 0x40,
    PLL1_MASHSEED0_OFF = 0x5,
    PLL1_MASHSEED0_MSK = 0x20,
    PLL1_DTHRMODE_OFF = 0x3,
    PLL1_DTHRMODE_MSK = 0x18,
    PLL1_ORDER_OFF = 0x0,
    PLL1_ORDER_MSK = 0x7,
};
#define MAKE_LMK05318_PLL1_MASHCTRL(pll1_dual_ph_en, pll1_mashseed1, pll1_mashseed0, pll1_dthrmode, pll1_order) MAKE_LMK05318_REG_WR(PLL1_MASHCTRL, \
    (((pll1_dual_ph_en) << PLL1_DUAL_PH_EN_OFF) & PLL1_DUAL_PH_EN_MSK) |  \
    (((pll1_mashseed1) << PLL1_MASHSEED1_OFF) & PLL1_MASHSEED1_MSK) |  \
    (((pll1_mashseed0) << PLL1_MASHSEED0_OFF) & PLL1_MASHSEED0_MSK) |  \
    (((pll1_dthrmode) << PLL1_DTHRMODE_OFF) & PLL1_DTHRMODE_MSK) |  \
    (((pll1_order) << PLL1_ORDER_OFF) & PLL1_ORDER_MSK))
// Register R116 [0x74] -- PLL1_MODE

enum pll1_mode_fields_t {
    PLL1_IGNORE_GPIO_PIN_OFF = 0x2,
    PLL1_IGNORE_GPIO_PIN_MSK = 0x4,
    PLL1_FDEV_EN_OFF = 0x1,
    PLL1_FDEV_EN_MSK = 0x2,
    PLL1_MODE_OFF = 0x0,
    PLL1_MODE_MSK = 0x1,
};
#define MAKE_LMK05318_PLL1_MODE(pll1_ignore_gpio_pin, pll1_fdev_en, pll1_mode) MAKE_LMK05318_REG_WR(PLL1_MODE, \
    (((pll1_ignore_gpio_pin) << PLL1_IGNORE_GPIO_PIN_OFF) & PLL1_IGNORE_GPIO_PIN_MSK) |  \
    (((pll1_fdev_en) << PLL1_FDEV_EN_OFF) & PLL1_FDEV_EN_MSK) |  \
    (((pll1_mode) << PLL1_MODE_OFF) & PLL1_MODE_MSK))
// Register R123 [0x7b] -- PLL1_NUM_STAT


#define MAKE_LMK05318_PLL1_NUM_STAT_BY0(value) MAKE_LMK05318_REG_WR(PLL1_NUM_STAT_BY0, (((value) >> 32) & 0xff))
#define MAKE_LMK05318_PLL1_NUM_STAT_BY1(value) MAKE_LMK05318_REG_WR(PLL1_NUM_STAT_BY1, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_PLL1_NUM_STAT_BY2(value) MAKE_LMK05318_REG_WR(PLL1_NUM_STAT_BY2, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_PLL1_NUM_STAT_BY3(value) MAKE_LMK05318_REG_WR(PLL1_NUM_STAT_BY3, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_PLL1_NUM_STAT_BY4(value) MAKE_LMK05318_REG_WR(PLL1_NUM_STAT_BY4, (((value) << 0) & 0xff))
// Register R129 [0x81] -- PLL1_LF_R2

enum pll1_lf_r2_fields_t {
    PLL1_LF_R2_OFF = 0x0,
    PLL1_LF_R2_MSK = 0x3f,
};
#define MAKE_LMK05318_PLL1_LF_R2(pll1_lf_r2) MAKE_LMK05318_REG_WR(PLL1_LF_R2, \
    (((pll1_lf_r2) << PLL1_LF_R2_OFF) & PLL1_LF_R2_MSK))
// Register R130 [0x82] -- PLL1_LF_C1

enum pll1_lf_c1_fields_t {
    PLL1_LF_C1_OFF = 0x0,
    PLL1_LF_C1_MSK = 0x7,
};
#define MAKE_LMK05318_PLL1_LF_C1(pll1_lf_c1) MAKE_LMK05318_REG_WR(PLL1_LF_C1, \
    (((pll1_lf_c1) << PLL1_LF_C1_OFF) & PLL1_LF_C1_MSK))
// Register R131 [0x83] -- PLL1_LF_R3

enum pll1_lf_r3_fields_t {
    PLL1_LF_R3_OFF = 0x0,
    PLL1_LF_R3_MSK = 0x3f,
};
#define MAKE_LMK05318_PLL1_LF_R3(pll1_lf_r3) MAKE_LMK05318_REG_WR(PLL1_LF_R3, \
    (((pll1_lf_r3) << PLL1_LF_R3_OFF) & PLL1_LF_R3_MSK))
// Register R132 [0x84] -- PLL1_LF_R4

enum pll1_lf_r4_fields_t {
    PLL1_LF_R4_OFF = 0x0,
    PLL1_LF_R4_MSK = 0x3f,
};
#define MAKE_LMK05318_PLL1_LF_R4(pll1_lf_r4) MAKE_LMK05318_REG_WR(PLL1_LF_R4, \
    (((pll1_lf_r4) << PLL1_LF_R4_OFF) & PLL1_LF_R4_MSK))
// Register R134 [0x86] -- PLL2_NDIV


#define MAKE_LMK05318_PLL2_NDIV_BY0(value) MAKE_LMK05318_REG_WR(PLL2_NDIV_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_PLL2_NDIV_BY1(value) MAKE_LMK05318_REG_WR(PLL2_NDIV_BY1, (((value) << 0) & 0xff))
// Register R136 [0x88] -- PLL2_NUM


#define MAKE_LMK05318_PLL2_NUM_BY0(value) MAKE_LMK05318_REG_WR(PLL2_NUM_BY0, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_PLL2_NUM_BY1(value) MAKE_LMK05318_REG_WR(PLL2_NUM_BY1, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_PLL2_NUM_BY2(value) MAKE_LMK05318_REG_WR(PLL2_NUM_BY2, (((value) << 0) & 0xff))
// Register R139 [0x8b] -- PLL2_MASHCTRL

enum pll2_mashctrl_fields_t {
    PLL2_DTHRMODE_OFF = 0x3,
    PLL2_DTHRMODE_MSK = 0x18,
    PLL2_ORDER_OFF = 0x0,
    PLL2_ORDER_MSK = 0x7,
};
#define MAKE_LMK05318_PLL2_MASHCTRL(pll2_dthrmode, pll2_order) MAKE_LMK05318_REG_WR(PLL2_MASHCTRL, \
    (((pll2_dthrmode) << PLL2_DTHRMODE_OFF) & PLL2_DTHRMODE_MSK) |  \
    (((pll2_order) << PLL2_ORDER_OFF) & PLL2_ORDER_MSK))
// Register R140 [0x8c] -- PLL2_LF_R2

enum pll2_lf_r2_fields_t {
    PLL2_LR_R2_OFF = 0x0,
    PLL2_LR_R2_MSK = 0x3f,
};
#define MAKE_LMK05318_PLL2_LF_R2(pll2_lr_r2) MAKE_LMK05318_REG_WR(PLL2_LF_R2, \
    (((pll2_lr_r2) << PLL2_LR_R2_OFF) & PLL2_LR_R2_MSK))
// Register R142 [0x8e] -- PLL2_LF_R3

enum pll2_lf_r3_fields_t {
    PLL2_LR_R3_OFF = 0x0,
    PLL2_LR_R3_MSK = 0x3f,
};
#define MAKE_LMK05318_PLL2_LF_R3(pll2_lr_r3) MAKE_LMK05318_REG_WR(PLL2_LF_R3, \
    (((pll2_lr_r3) << PLL2_LR_R3_OFF) & PLL2_LR_R3_MSK))
// Register R143 [0x8f] -- PLL2_LF_R4

enum pll2_lf_r4_fields_t {
    PLL2_LF_R4_OFF = 0x0,
    PLL2_LF_R4_MSK = 0x3f,
};
#define MAKE_LMK05318_PLL2_LF_R4(pll2_lf_r4) MAKE_LMK05318_REG_WR(PLL2_LF_R4, \
    (((pll2_lf_r4) << PLL2_LF_R4_OFF) & PLL2_LF_R4_MSK))
// Register R144 [0x90] -- PLL2_LF_C3C4

enum pll2_lf_c3c4_fields_t {
    PLL2_LF_C4_OFF = 0x4,
    PLL2_LF_C4_MSK = 0x70,
    PLL2_LF_C3_OFF = 0x0,
    PLL2_LF_C3_MSK = 0x7,
};
#define MAKE_LMK05318_PLL2_LF_C3C4(pll2_lf_c4, pll2_lf_c3) MAKE_LMK05318_REG_WR(PLL2_LF_C3C4, \
    (((pll2_lf_c4) << PLL2_LF_C4_OFF) & PLL2_LF_C4_MSK) |  \
    (((pll2_lf_c3) << PLL2_LF_C3_OFF) & PLL2_LF_C3_MSK))
// Register R145 [0x91] -- XO_OFFSET_SW_TIMER

enum xo_offset_sw_timer_fields_t {
    XO_TIMER_OFF = 0x0,
    XO_TIMER_MSK = 0x7,
};
#define MAKE_LMK05318_XO_OFFSET_SW_TIMER(xo_timer) MAKE_LMK05318_REG_WR(XO_OFFSET_SW_TIMER, \
    (((xo_timer) << XO_TIMER_OFF) & XO_TIMER_MSK))
// Register R180 [0xb4] -- DPLL_TUNING_FREE_RUN


#define MAKE_LMK05318_DPLL_TUNING_FREE_RUN_BY0(value) MAKE_LMK05318_REG_WR(DPLL_TUNING_FREE_RUN_BY0, (((value) >> 32) & 0xff))
#define MAKE_LMK05318_DPLL_TUNING_FREE_RUN_BY1(value) MAKE_LMK05318_REG_WR(DPLL_TUNING_FREE_RUN_BY1, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_DPLL_TUNING_FREE_RUN_BY2(value) MAKE_LMK05318_REG_WR(DPLL_TUNING_FREE_RUN_BY2, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_TUNING_FREE_RUN_BY3(value) MAKE_LMK05318_REG_WR(DPLL_TUNING_FREE_RUN_BY3, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_TUNING_FREE_RUN_BY4(value) MAKE_LMK05318_REG_WR(DPLL_TUNING_FREE_RUN_BY4, (((value) << 0) & 0xff))
// Register R185 [0xb9] -- DPLL_REF_HISTCTL

// Register R186 [0xba] -- DPLL_REF_HISTCNT

// Register R187 [0xbb] -- DPLL_REF_HISTDLY


#define MAKE_LMK05318_DPLL_REF_HISTDLY_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_HISTDLY_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_DPLL_REF_HISTDLY_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_HISTDLY_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_REF_HISTDLY_BY2(value) MAKE_LMK05318_REG_WR(DPLL_REF_HISTDLY_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_HISTDLY_BY3(value) MAKE_LMK05318_REG_WR(DPLL_REF_HISTDLY_BY3, (((value) << 0) & 0xff))
// Register R192 [0xc0] -- REF01_DETAMP

enum ref01_detamp_fields_t {
    DETECT_MODE_SECREF_OFF = 0x6,
    DETECT_MODE_SECREF_MSK = 0xc0,
    DETECT_MODE_PRIREF_OFF = 0x4,
    DETECT_MODE_PRIREF_MSK = 0x30,
    SECREF_LVL_SEL_OFF = 0x2,
    SECREF_LVL_SEL_MSK = 0xc,
    PRIREF_LVL_SEL_OFF = 0x0,
    PRIREF_LVL_SEL_MSK = 0x3,
};
#define MAKE_LMK05318_REF01_DETAMP(detect_mode_secref, detect_mode_priref, secref_lvl_sel, priref_lvl_sel) MAKE_LMK05318_REG_WR(REF01_DETAMP, \
    (((detect_mode_secref) << DETECT_MODE_SECREF_OFF) & DETECT_MODE_SECREF_MSK) |  \
    (((detect_mode_priref) << DETECT_MODE_PRIREF_OFF) & DETECT_MODE_PRIREF_MSK) |  \
    (((secref_lvl_sel) << SECREF_LVL_SEL_OFF) & SECREF_LVL_SEL_MSK) |  \
    (((priref_lvl_sel) << PRIREF_LVL_SEL_OFF) & PRIREF_LVL_SEL_MSK))
// Register R193 [0xc1] -- REF0_DETEN

enum ref0_deten_fields_t {
    PRIREF_EARLY_DET_EN_OFF = 0x5,
    PRIREF_EARLY_DET_EN_MSK = 0x20,
    PRIREF_PH_VALID_EN_OFF = 0x4,
    PRIREF_PH_VALID_EN_MSK = 0x10,
    PRIREF_VALTMR_EN_OFF = 0x3,
    PRIREF_VALTMR_EN_MSK = 0x8,
    PRIREF_PPM_EN_OFF = 0x2,
    PRIREF_PPM_EN_MSK = 0x4,
    PRIREF_MISSCLK_EN_OFF = 0x1,
    PRIREF_MISSCLK_EN_MSK = 0x2,
    PRIREF_AMPDET_EN_OFF = 0x0,
    PRIREF_AMPDET_EN_MSK = 0x1,
};
#define MAKE_LMK05318_REF0_DETEN(priref_early_det_en, priref_ph_valid_en, priref_valtmr_en, priref_ppm_en, priref_missclk_en, priref_ampdet_en) MAKE_LMK05318_REG_WR(REF0_DETEN, \
    (((priref_early_det_en) << PRIREF_EARLY_DET_EN_OFF) & PRIREF_EARLY_DET_EN_MSK) |  \
    (((priref_ph_valid_en) << PRIREF_PH_VALID_EN_OFF) & PRIREF_PH_VALID_EN_MSK) |  \
    (((priref_valtmr_en) << PRIREF_VALTMR_EN_OFF) & PRIREF_VALTMR_EN_MSK) |  \
    (((priref_ppm_en) << PRIREF_PPM_EN_OFF) & PRIREF_PPM_EN_MSK) |  \
    (((priref_missclk_en) << PRIREF_MISSCLK_EN_OFF) & PRIREF_MISSCLK_EN_MSK) |  \
    (((priref_ampdet_en) << PRIREF_AMPDET_EN_OFF) & PRIREF_AMPDET_EN_MSK))
// Register R194 [0xc2] -- REF1_DETEN

enum ref1_deten_fields_t {
    SECREF_EARLY_DET_EN_OFF = 0x5,
    SECREF_EARLY_DET_EN_MSK = 0x20,
    SECREF_PH_VALID_EN_OFF = 0x4,
    SECREF_PH_VALID_EN_MSK = 0x10,
    SECREF_VALTMR_EN_OFF = 0x3,
    SECREF_VALTMR_EN_MSK = 0x8,
    SECREF_PPM_EN_OFF = 0x2,
    SECREF_PPM_EN_MSK = 0x4,
    SECREF_MISSCLK_EN_OFF = 0x1,
    SECREF_MISSCLK_EN_MSK = 0x2,
    SECREF_AMPDET_EN_OFF = 0x0,
    SECREF_AMPDET_EN_MSK = 0x1,
};
#define MAKE_LMK05318_REF1_DETEN(secref_early_det_en, secref_ph_valid_en, secref_valtmr_en, secref_ppm_en, secref_missclk_en, secref_ampdet_en) MAKE_LMK05318_REG_WR(REF1_DETEN, \
    (((secref_early_det_en) << SECREF_EARLY_DET_EN_OFF) & SECREF_EARLY_DET_EN_MSK) |  \
    (((secref_ph_valid_en) << SECREF_PH_VALID_EN_OFF) & SECREF_PH_VALID_EN_MSK) |  \
    (((secref_valtmr_en) << SECREF_VALTMR_EN_OFF) & SECREF_VALTMR_EN_MSK) |  \
    (((secref_ppm_en) << SECREF_PPM_EN_OFF) & SECREF_PPM_EN_MSK) |  \
    (((secref_missclk_en) << SECREF_MISSCLK_EN_OFF) & SECREF_MISSCLK_EN_MSK) |  \
    (((secref_ampdet_en) << SECREF_AMPDET_EN_OFF) & SECREF_AMPDET_EN_MSK))
// Register R195 [0xc3] -- REF0_MISSCLK_DIV


#define MAKE_LMK05318_REF0_MISSCLK_DIV_BY0(value) MAKE_LMK05318_REG_WR(REF0_MISSCLK_DIV_BY0, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_REF0_MISSCLK_DIV_BY1(value) MAKE_LMK05318_REG_WR(REF0_MISSCLK_DIV_BY1, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF0_MISSCLK_DIV_BY2(value) MAKE_LMK05318_REG_WR(REF0_MISSCLK_DIV_BY2, (((value) << 0) & 0xff))
// Register R198 [0xc6] -- REF1_MISSCLK_DIV


#define MAKE_LMK05318_REF1_MISSCLK_DIV_BY0(value) MAKE_LMK05318_REG_WR(REF1_MISSCLK_DIV_BY0, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_REF1_MISSCLK_DIV_BY1(value) MAKE_LMK05318_REG_WR(REF1_MISSCLK_DIV_BY1, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF1_MISSCLK_DIV_BY2(value) MAKE_LMK05318_REG_WR(REF1_MISSCLK_DIV_BY2, (((value) << 0) & 0xff))
// Register R201 [0xc9] -- REF_MISSCLK_CTL

enum ref_missclk_ctl_fields_t {
    SECREF_WINDOW_DET_OFF = 0x1,
    SECREF_WINDOW_DET_MSK = 0x2,
    PRIREF_WINDOW_DET_OFF = 0x0,
    PRIREF_WINDOW_DET_MSK = 0x1,
};
#define MAKE_LMK05318_REF_MISSCLK_CTL(secref_window_det, priref_window_det) MAKE_LMK05318_REG_WR(REF_MISSCLK_CTL, \
    (((secref_window_det) << SECREF_WINDOW_DET_OFF) & SECREF_WINDOW_DET_MSK) |  \
    (((priref_window_det) << PRIREF_WINDOW_DET_OFF) & PRIREF_WINDOW_DET_MSK))
// Register R202 [0xca] -- REF0_EARLY_CLK_DIV


#define MAKE_LMK05318_REF0_EARLY_CLK_DIV_BY0(value) MAKE_LMK05318_REG_WR(REF0_EARLY_CLK_DIV_BY0, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_REF0_EARLY_CLK_DIV_BY1(value) MAKE_LMK05318_REG_WR(REF0_EARLY_CLK_DIV_BY1, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF0_EARLY_CLK_DIV_BY2(value) MAKE_LMK05318_REG_WR(REF0_EARLY_CLK_DIV_BY2, (((value) << 0) & 0xff))
// Register R205 [0xcd] -- REF1_EARLY_CLK_DIV


#define MAKE_LMK05318_REF1_EARLY_CLK_DIV_BY0(value) MAKE_LMK05318_REG_WR(REF1_EARLY_CLK_DIV_BY0, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_REF1_EARLY_CLK_DIV_BY1(value) MAKE_LMK05318_REG_WR(REF1_EARLY_CLK_DIV_BY1, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF1_EARLY_CLK_DIV_BY2(value) MAKE_LMK05318_REG_WR(REF1_EARLY_CLK_DIV_BY2, (((value) << 0) & 0xff))
// Register R208 [0xd0] -- REF0_PPM_MIN


#define MAKE_LMK05318_REF0_PPM_MIN_BY0(value) MAKE_LMK05318_REG_WR(REF0_PPM_MIN_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF0_PPM_MIN_BY1(value) MAKE_LMK05318_REG_WR(REF0_PPM_MIN_BY1, (((value) << 0) & 0xff))
// Register R210 [0xd2] -- REF0_PPM_MAX


#define MAKE_LMK05318_REF0_PPM_MAX_BY0(value) MAKE_LMK05318_REG_WR(REF0_PPM_MAX_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF0_PPM_MAX_BY1(value) MAKE_LMK05318_REG_WR(REF0_PPM_MAX_BY1, (((value) << 0) & 0xff))
// Register R212 [0xd4] -- REF1_PPM_MIN


#define MAKE_LMK05318_REF1_PPM_MIN_BY0(value) MAKE_LMK05318_REG_WR(REF1_PPM_MIN_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF1_PPM_MIN_BY1(value) MAKE_LMK05318_REG_WR(REF1_PPM_MIN_BY1, (((value) << 0) & 0xff))
// Register R214 [0xd6] -- REF1_PPM_MAX


#define MAKE_LMK05318_REF1_PPM_MAX_BY0(value) MAKE_LMK05318_REG_WR(REF1_PPM_MAX_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF1_PPM_MAX_BY1(value) MAKE_LMK05318_REG_WR(REF1_PPM_MAX_BY1, (((value) << 0) & 0xff))
// Register R217 [0xd9] -- REF0_CNTSTRT


#define MAKE_LMK05318_REF0_CNTSTRT_BY0(value) MAKE_LMK05318_REG_WR(REF0_CNTSTRT_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_REF0_CNTSTRT_BY1(value) MAKE_LMK05318_REG_WR(REF0_CNTSTRT_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_REF0_CNTSTRT_BY2(value) MAKE_LMK05318_REG_WR(REF0_CNTSTRT_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF0_CNTSTRT_BY3(value) MAKE_LMK05318_REG_WR(REF0_CNTSTRT_BY3, (((value) << 0) & 0xff))
// Register R221 [0xdd] -- REF0_HOLD_CNTSTRT


#define MAKE_LMK05318_REF0_HOLD_CNTSTRT_BY0(value) MAKE_LMK05318_REG_WR(REF0_HOLD_CNTSTRT_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_REF0_HOLD_CNTSTRT_BY1(value) MAKE_LMK05318_REG_WR(REF0_HOLD_CNTSTRT_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_REF0_HOLD_CNTSTRT_BY2(value) MAKE_LMK05318_REG_WR(REF0_HOLD_CNTSTRT_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF0_HOLD_CNTSTRT_BY3(value) MAKE_LMK05318_REG_WR(REF0_HOLD_CNTSTRT_BY3, (((value) << 0) & 0xff))
// Register R225 [0xe1] -- REF1_CNTSTRT


#define MAKE_LMK05318_REF1_CNTSTRT_BY0(value) MAKE_LMK05318_REG_WR(REF1_CNTSTRT_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_REF1_CNTSTRT_BY1(value) MAKE_LMK05318_REG_WR(REF1_CNTSTRT_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_REF1_CNTSTRT_BY2(value) MAKE_LMK05318_REG_WR(REF1_CNTSTRT_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF1_CNTSTRT_BY3(value) MAKE_LMK05318_REG_WR(REF1_CNTSTRT_BY3, (((value) << 0) & 0xff))
// Register R229 [0xe5] -- REF1_HOLD_CNTSTRT


#define MAKE_LMK05318_REF1_HOLD_CNTSTRT_BY0(value) MAKE_LMK05318_REG_WR(REF1_HOLD_CNTSTRT_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_REF1_HOLD_CNTSTRT_BY1(value) MAKE_LMK05318_REG_WR(REF1_HOLD_CNTSTRT_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_REF1_HOLD_CNTSTRT_BY2(value) MAKE_LMK05318_REG_WR(REF1_HOLD_CNTSTRT_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF1_HOLD_CNTSTRT_BY3(value) MAKE_LMK05318_REG_WR(REF1_HOLD_CNTSTRT_BY3, (((value) << 0) & 0xff))
// Register R233 [0xe9] -- REF0_VLDTMR

// Register R234 [0xea] -- REF1_VLDTMR

// Register R235 [0xeb] -- REF0_PH_VALID_CNT


#define MAKE_LMK05318_REF0_PH_VALID_CNT_BY0(value) MAKE_LMK05318_REG_WR(REF0_PH_VALID_CNT_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_REF0_PH_VALID_CNT_BY1(value) MAKE_LMK05318_REG_WR(REF0_PH_VALID_CNT_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_REF0_PH_VALID_CNT_BY2(value) MAKE_LMK05318_REG_WR(REF0_PH_VALID_CNT_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF0_PH_VALID_CNT_BY3(value) MAKE_LMK05318_REG_WR(REF0_PH_VALID_CNT_BY3, (((value) << 0) & 0xff))
// Register R239 [0xef] -- REF1_PH_VALID_CNT


#define MAKE_LMK05318_REF1_PH_VALID_CNT_BY0(value) MAKE_LMK05318_REG_WR(REF1_PH_VALID_CNT_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_REF1_PH_VALID_CNT_BY1(value) MAKE_LMK05318_REG_WR(REF1_PH_VALID_CNT_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_REF1_PH_VALID_CNT_BY2(value) MAKE_LMK05318_REG_WR(REF1_PH_VALID_CNT_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_REF1_PH_VALID_CNT_BY3(value) MAKE_LMK05318_REG_WR(REF1_PH_VALID_CNT_BY3, (((value) << 0) & 0xff))
// Register R243 [0xf3] -- REF0_PH_VALID_THR

enum ref0_ph_valid_thr_fields_t {
    REF0_PH_VALID_THR_OFF = 0x0,
    REF0_PH_VALID_THR_MSK = 0x3f,
};
#define MAKE_LMK05318_REF0_PH_VALID_THR(ref0_ph_valid_thr) MAKE_LMK05318_REG_WR(REF0_PH_VALID_THR, \
    (((ref0_ph_valid_thr) << REF0_PH_VALID_THR_OFF) & REF0_PH_VALID_THR_MSK))
// Register R244 [0xf4] -- REF1_PH_VALID_THR

enum ref1_ph_valid_thr_fields_t {
    REF1_PH_VALID_THR_OFF = 0x0,
    REF1_PH_VALID_THR_MSK = 0x3f,
};
#define MAKE_LMK05318_REF1_PH_VALID_THR(ref1_ph_valid_thr) MAKE_LMK05318_REG_WR(REF1_PH_VALID_THR, \
    (((ref1_ph_valid_thr) << REF1_PH_VALID_THR_OFF) & REF1_PH_VALID_THR_MSK))
// Register R249 [0xf9] -- DPLL_REF01_PRTY

enum dpll_ref01_prty_fields_t {
    DPLL_SECREF_AUTO_PRTY_OFF = 0x4,
    DPLL_SECREF_AUTO_PRTY_MSK = 0x30,
    DPLL_PRIREF_AUTO_PRTY_OFF = 0x0,
    DPLL_PRIREF_AUTO_PRTY_MSK = 0x3,
};
#define MAKE_LMK05318_DPLL_REF01_PRTY(dpll_secref_auto_prty, dpll_priref_auto_prty) MAKE_LMK05318_REG_WR(DPLL_REF01_PRTY, \
    (((dpll_secref_auto_prty) << DPLL_SECREF_AUTO_PRTY_OFF) & DPLL_SECREF_AUTO_PRTY_MSK) |  \
    (((dpll_priref_auto_prty) << DPLL_PRIREF_AUTO_PRTY_OFF) & DPLL_PRIREF_AUTO_PRTY_MSK))
// Register R251 [0xfb] -- DPLL_REF_SWMODE

enum dpll_ref_swmode_fields_t {
    DPLL_REF_MAN_SEL_OFF = 0x5,
    DPLL_REF_MAN_SEL_MSK = 0x20,
    DPLL_REF_MAN_REG_SEL_OFF = 0x4,
    DPLL_REF_MAN_REG_SEL_MSK = 0x10,
    DPLL_SWITCH_MODE_OFF = 0x0,
    DPLL_SWITCH_MODE_MSK = 0x3,
};
#define MAKE_LMK05318_DPLL_REF_SWMODE(dpll_ref_man_sel, dpll_ref_man_reg_sel, dpll_switch_mode) MAKE_LMK05318_REG_WR(DPLL_REF_SWMODE, \
    (((dpll_ref_man_sel) << DPLL_REF_MAN_SEL_OFF) & DPLL_REF_MAN_SEL_MSK) |  \
    (((dpll_ref_man_reg_sel) << DPLL_REF_MAN_REG_SEL_OFF) & DPLL_REF_MAN_REG_SEL_MSK) |  \
    (((dpll_switch_mode) << DPLL_SWITCH_MODE_OFF) & DPLL_SWITCH_MODE_MSK))
// Register R252 [0xfc] -- DPLL_GEN_CTL

enum dpll_gen_ctl_fields_t {
    DPLL_ZDM_SYNC_EN_OFF = 0x7,
    DPLL_ZDM_SYNC_EN_MSK = 0x80,
    DPLL_ZDM_NDIV_RST_DIS_OFF = 0x6,
    DPLL_ZDM_NDIV_RST_DIS_MSK = 0x40,
    DPLL_SWITCHOVER_1_OFF = 0x5,
    DPLL_SWITCHOVER_1_MSK = 0x20,
    DPLL_FASTLOCK_ALWAYS_OFF = 0x4,
    DPLL_FASTLOCK_ALWAYS_MSK = 0x10,
    DPLL_LOCKDET_PPM_EN_OFF = 0x3,
    DPLL_LOCKDET_PPM_EN_MSK = 0x8,
    DPLL_HLDOVR_MODE_OFF = 0x2,
    DPLL_HLDOVR_MODE_MSK = 0x4,
    DPLL_LOOP_EN_OFF = 0x0,
    DPLL_LOOP_EN_MSK = 0x1,
};
#define MAKE_LMK05318_DPLL_GEN_CTL(dpll_zdm_sync_en, dpll_zdm_ndiv_rst_dis, dpll_switchover_1, dpll_fastlock_always, dpll_lockdet_ppm_en, dpll_hldovr_mode, dpll_loop_en) MAKE_LMK05318_REG_WR(DPLL_GEN_CTL, \
    (((dpll_zdm_sync_en) << DPLL_ZDM_SYNC_EN_OFF) & DPLL_ZDM_SYNC_EN_MSK) |  \
    (((dpll_zdm_ndiv_rst_dis) << DPLL_ZDM_NDIV_RST_DIS_OFF) & DPLL_ZDM_NDIV_RST_DIS_MSK) |  \
    (((dpll_switchover_1) << DPLL_SWITCHOVER_1_OFF) & DPLL_SWITCHOVER_1_MSK) |  \
    (((dpll_fastlock_always) << DPLL_FASTLOCK_ALWAYS_OFF) & DPLL_FASTLOCK_ALWAYS_MSK) |  \
    (((dpll_lockdet_ppm_en) << DPLL_LOCKDET_PPM_EN_OFF) & DPLL_LOCKDET_PPM_EN_MSK) |  \
    (((dpll_hldovr_mode) << DPLL_HLDOVR_MODE_OFF) & DPLL_HLDOVR_MODE_MSK) |  \
    (((dpll_loop_en) << DPLL_LOOP_EN_OFF) & DPLL_LOOP_EN_MSK))
// Register R253 [0xfd] -- DPLL_SWITCHOVER_TMR_EXP

enum dpll_switchover_tmr_exp_fields_t {
    DPLL_SWITCHOVER_2_OFF = 0x0,
    DPLL_SWITCHOVER_2_MSK = 0x1f,
};
#define MAKE_LMK05318_DPLL_SWITCHOVER_TMR_EXP(dpll_switchover_2) MAKE_LMK05318_REG_WR(DPLL_SWITCHOVER_TMR_EXP, \
    (((dpll_switchover_2) << DPLL_SWITCHOVER_2_OFF) & DPLL_SWITCHOVER_2_MSK))
// Register R254 [0xfe] -- DPLL_SWITCHOVER_TMR_MANT_BY1

enum dpll_switchover_tmr_mant_by1_fields_t {
    DPLL_SWITCHOVER_3_OFF = 0x0,
    DPLL_SWITCHOVER_3_MSK = 0x7,
};
#define MAKE_LMK05318_DPLL_SWITCHOVER_TMR_MANT_BY1(dpll_switchover_3) MAKE_LMK05318_REG_WR(DPLL_SWITCHOVER_TMR_MANT_BY1, \
    (((dpll_switchover_3) << DPLL_SWITCHOVER_3_OFF) & DPLL_SWITCHOVER_3_MSK))
// Register R255 [0xff] -- DPLL_SWITCHOVER_TMR_MANT_BY0

// Register R256 [0x100] -- DPLL_REF0_RDIV


#define MAKE_LMK05318_DPLL_REF0_RDIV_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF0_RDIV_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF0_RDIV_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF0_RDIV_BY1, (((value) << 0) & 0xff))
// Register R258 [0x102] -- DPLL_REF1_RDIV


#define MAKE_LMK05318_DPLL_REF1_RDIV_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF1_RDIV_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF1_RDIV_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF1_RDIV_BY1, (((value) << 0) & 0xff))
// Register R260 [0x104] -- DPLL_REF_TDC_CTL

enum dpll_ref_tdc_ctl_fields_t {
    DPLL_TDC_SW_MODE_OFF = 0x4,
    DPLL_TDC_SW_MODE_MSK = 0x10,
    DPLL_REF_AVOID_SLIP_OFF = 0x1,
    DPLL_REF_AVOID_SLIP_MSK = 0x2,
};
#define MAKE_LMK05318_DPLL_REF_TDC_CTL(dpll_tdc_sw_mode, dpll_ref_avoid_slip) MAKE_LMK05318_REG_WR(DPLL_REF_TDC_CTL, \
    (((dpll_tdc_sw_mode) << DPLL_TDC_SW_MODE_OFF) & DPLL_TDC_SW_MODE_MSK) |  \
    (((dpll_ref_avoid_slip) << DPLL_REF_AVOID_SLIP_OFF) & DPLL_REF_AVOID_SLIP_MSK))
// Register R261 [0x105] -- DPLL_REF_DLY_GEN

// Register R262 [0x106] -- DPLL_REF_CYCSLIP_OFFSET


#define MAKE_LMK05318_DPLL_REF_CYCSLIP_OFFSET_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_CYCSLIP_OFFSET_BY0, (((value) >> 32) & 0xff))
#define MAKE_LMK05318_DPLL_REF_CYCSLIP_OFFSET_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_CYCSLIP_OFFSET_BY1, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_DPLL_REF_CYCSLIP_OFFSET_BY2(value) MAKE_LMK05318_REG_WR(DPLL_REF_CYCSLIP_OFFSET_BY2, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_REF_CYCSLIP_OFFSET_BY3(value) MAKE_LMK05318_REG_WR(DPLL_REF_CYCSLIP_OFFSET_BY3, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_CYCSLIP_OFFSET_BY4(value) MAKE_LMK05318_REG_WR(DPLL_REF_CYCSLIP_OFFSET_BY4, (((value) << 0) & 0xff))
// Register R267 [0x10b] -- DPLL_REF_LOOPCTL

// Register R268 [0x10c] -- DPLL_REF_LOOPCTL_CHG

// Register R269 [0x10d] -- DPLL_REF_DECIMATION

// Register R270 [0x10e] -- DPLL_REF_FILTSCALAR


#define MAKE_LMK05318_DPLL_REF_FILTSCALAR_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_FILTSCALAR_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_FILTSCALAR_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_FILTSCALAR_BY1, (((value) << 0) & 0xff))
// Register R272 [0x110] -- DPLL_REF_FILTGAIN

// Register R273 [0x111] -- DPLL_REF_FILTGAIN_FL1

// Register R274 [0x112] -- DPLL_REF_FILTGAIN_FL2

// Register R275 [0x113] -- DPLL_REF_LOOPGAIN

// Register R276 [0x114] -- DPLL_REF_LOOPGAIN_FL1

// Register R277 [0x115] -- DPLL_REF_LOOPGAIN_FL2

// Register R278 [0x116] -- DPLL_REF_LPF0GAIN

// Register R279 [0x117] -- DPLL_REF_LPF0GAIN_FL1

// Register R280 [0x118] -- DPLL_REF_LPF0GAIN_FL2

// Register R281 [0x119] -- DPLL_REF_LPF1GAIN

// Register R282 [0x11a] -- DPLL_REF_LPF1GAIN_FL1

// Register R283 [0x11b] -- DPLL_REF_LPF1GAIN_FL2

// Register R284 [0x11c] -- DPLL_REF_LPF0GAIN2_FL

// Register R285 [0x11d] -- DPLL_REF_LPF1GAIN2_FL

// Register R286 [0x11e] -- DPLL_REF_TMR_FL1


#define MAKE_LMK05318_DPLL_REF_TMR_FL1_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_TMR_FL1_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_TMR_FL1_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_TMR_FL1_BY1, (((value) << 0) & 0xff))
// Register R288 [0x120] -- DPLL_REF_TMR_FL2


#define MAKE_LMK05318_DPLL_REF_TMR_FL2_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_TMR_FL2_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_TMR_FL2_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_TMR_FL2_BY1, (((value) << 0) & 0xff))
// Register R290 [0x122] -- DPLL_REF_TMR_LCK


#define MAKE_LMK05318_DPLL_REF_TMR_LCK_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_TMR_LCK_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_TMR_LCK_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_TMR_LCK_BY1, (((value) << 0) & 0xff))
// Register R292 [0x124] -- DPLL_REF_PHC_LPF

// Register R293 [0x125] -- DPLL_REF_PHC_CTRL

// Register R294 [0x126] -- DPLL_REF_PHC_TIMER


#define MAKE_LMK05318_DPLL_REF_PHC_TIMER_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_PHC_TIMER_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_PHC_TIMER_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_PHC_TIMER_BY1, (((value) << 0) & 0xff))
// Register R296 [0x128] -- DPLL_REF_QUANT

// Register R297 [0x129] -- DPLL_REF_QUANT_FL1

// Register R298 [0x12a] -- DPLL_REF_QUANT_FL2

// Register R300 [0x12c] -- DPLL_PL_LPF_GAIN

// Register R301 [0x12d] -- DPLL_PL_THRESH

// Register R302 [0x12e] -- DPLL_PL_UNLK_THRESH

// Register R304 [0x130] -- DPLL_REF_FB_PREDIV

enum dpll_ref_fb_prediv_fields_t {
    DPLL_REF_FB_PRE_DIV_OFF = 0x0,
    DPLL_REF_FB_PRE_DIV_MSK = 0xf,
};
#define MAKE_LMK05318_DPLL_REF_FB_PREDIV(dpll_ref_fb_pre_div) MAKE_LMK05318_REG_WR(DPLL_REF_FB_PREDIV, \
    (((dpll_ref_fb_pre_div) << DPLL_REF_FB_PRE_DIV_OFF) & DPLL_REF_FB_PRE_DIV_MSK))
// Register R305 [0x131] -- DPLL_REF_FB_DIV


#define MAKE_LMK05318_DPLL_REF_FB_DIV_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_FB_DIV_BY0, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_DPLL_REF_FB_DIV_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_FB_DIV_BY1, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_REF_FB_DIV_BY2(value) MAKE_LMK05318_REG_WR(DPLL_REF_FB_DIV_BY2, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_FB_DIV_BY3(value) MAKE_LMK05318_REG_WR(DPLL_REF_FB_DIV_BY3, (((value) << 0) & 0xff))
// Register R309 [0x135] -- DPLL_REF_NUM


#define MAKE_LMK05318_DPLL_REF_NUM_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_NUM_BY0, (((value) >> 32) & 0xff))
#define MAKE_LMK05318_DPLL_REF_NUM_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_NUM_BY1, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_DPLL_REF_NUM_BY2(value) MAKE_LMK05318_REG_WR(DPLL_REF_NUM_BY2, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_REF_NUM_BY3(value) MAKE_LMK05318_REG_WR(DPLL_REF_NUM_BY3, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_NUM_BY4(value) MAKE_LMK05318_REG_WR(DPLL_REF_NUM_BY4, (((value) << 0) & 0xff))
// Register R314 [0x13a] -- DPLL_REF_DEN


#define MAKE_LMK05318_DPLL_REF_DEN_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_DEN_BY0, (((value) >> 32) & 0xff))
#define MAKE_LMK05318_DPLL_REF_DEN_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_DEN_BY1, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_DPLL_REF_DEN_BY2(value) MAKE_LMK05318_REG_WR(DPLL_REF_DEN_BY2, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_REF_DEN_BY3(value) MAKE_LMK05318_REG_WR(DPLL_REF_DEN_BY3, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_DEN_BY4(value) MAKE_LMK05318_REG_WR(DPLL_REF_DEN_BY4, (((value) << 0) & 0xff))
// Register R319 [0x13f] -- DPLL_REF_MASHCTL

enum dpll_ref_mashctl_fields_t {
    DPLL_REF_DTHRMODE_OFF = 0x3,
    DPLL_REF_DTHRMODE_MSK = 0x18,
    DPLL_REF_ORDER_OFF = 0x0,
    DPLL_REF_ORDER_MSK = 0x7,
};
#define MAKE_LMK05318_DPLL_REF_MASHCTL(dpll_ref_dthrmode, dpll_ref_order) MAKE_LMK05318_REG_WR(DPLL_REF_MASHCTL, \
    (((dpll_ref_dthrmode) << DPLL_REF_DTHRMODE_OFF) & DPLL_REF_DTHRMODE_MSK) |  \
    (((dpll_ref_order) << DPLL_REF_ORDER_OFF) & DPLL_REF_ORDER_MSK))
// Register R320 [0x140] -- DPLL_REF_LOCKDET_1_5


#define MAKE_LMK05318_DPLL_REF_LOCKDET_1_5_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_LOCKDET_1_5_BY0, (((value) >> 32) & 0xff))
#define MAKE_LMK05318_DPLL_REF_LOCKDET_1_5_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_LOCKDET_1_5_BY1, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_DPLL_REF_LOCKDET_1_5_BY2(value) MAKE_LMK05318_REG_WR(DPLL_REF_LOCKDET_1_5_BY2, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_REF_LOCKDET_1_5_BY3(value) MAKE_LMK05318_REG_WR(DPLL_REF_LOCKDET_1_5_BY3, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_LOCKDET_1_5_BY4(value) MAKE_LMK05318_REG_WR(DPLL_REF_LOCKDET_1_5_BY4, (((value) << 0) & 0xff))
// Register R325 [0x145] -- DPLL_REF_LOCKDET_6_10


#define MAKE_LMK05318_DPLL_REF_LOCKDET_6_10_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_LOCKDET_6_10_BY0, (((value) >> 32) & 0xff))
#define MAKE_LMK05318_DPLL_REF_LOCKDET_6_10_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_LOCKDET_6_10_BY1, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_DPLL_REF_LOCKDET_6_10_BY2(value) MAKE_LMK05318_REG_WR(DPLL_REF_LOCKDET_6_10_BY2, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_REF_LOCKDET_6_10_BY3(value) MAKE_LMK05318_REG_WR(DPLL_REF_LOCKDET_6_10_BY3, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_LOCKDET_6_10_BY4(value) MAKE_LMK05318_REG_WR(DPLL_REF_LOCKDET_6_10_BY4, (((value) << 0) & 0xff))
// Register R330 [0x14a] -- DPLL_REF_UNLOCKDET_1_3


#define MAKE_LMK05318_DPLL_REF_UNLOCKDET_1_3_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_UNLOCKDET_1_3_BY0, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_REF_UNLOCKDET_1_3_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_UNLOCKDET_1_3_BY1, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_UNLOCKDET_1_3_BY2(value) MAKE_LMK05318_REG_WR(DPLL_REF_UNLOCKDET_1_3_BY2, (((value) << 0) & 0xff))
// Register R333 [0x14d] -- PLL2_DEN


#define MAKE_LMK05318_PLL2_DEN_BY0(value) MAKE_LMK05318_REG_WR(PLL2_DEN_BY0, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_PLL2_DEN_BY1(value) MAKE_LMK05318_REG_WR(PLL2_DEN_BY1, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_PLL2_DEN_BY2(value) MAKE_LMK05318_REG_WR(PLL2_DEN_BY2, (((value) << 0) & 0xff))
// Register R336 [0x150] -- DPLL_REF_UNLOCKDET_VCO_CNTSTRT


#define MAKE_LMK05318_DPLL_REF_UNLOCKDET_VCO_CNTSTRT_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_UNLOCKDET_VCO_CNTSTRT_BY0, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_REF_UNLOCKDET_VCO_CNTSTRT_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_UNLOCKDET_VCO_CNTSTRT_BY1, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_UNLOCKDET_VCO_CNTSTRT_BY2(value) MAKE_LMK05318_REG_WR(DPLL_REF_UNLOCKDET_VCO_CNTSTRT_BY2, (((value) << 0) & 0xff))
// Register R339 [0x153] -- PLL1_24B_NUM_23_16

enum pll1_24b_num_23_16_fields_t {
    PLL1_24B_NUM_23_16_OFF = 0x0,
    PLL1_24B_NUM_23_16_MSK = 0xff,
};
#define MAKE_LMK05318_PLL1_24B_NUM_23_16(pll1_24b_num_23_16) MAKE_LMK05318_REG_WR(PLL1_24B_NUM_23_16, \
    (((pll1_24b_num_23_16) << PLL1_24B_NUM_23_16_OFF) & PLL1_24B_NUM_23_16_MSK))
// Register R340 [0x154] -- DPLL_REF_SYNC_PH_OFFSET


#define MAKE_LMK05318_DPLL_REF_SYNC_PH_OFFSET_BY0(value) MAKE_LMK05318_REG_WR(DPLL_REF_SYNC_PH_OFFSET_BY0, (((value) >> 40) & 0xff))
#define MAKE_LMK05318_DPLL_REF_SYNC_PH_OFFSET_BY1(value) MAKE_LMK05318_REG_WR(DPLL_REF_SYNC_PH_OFFSET_BY1, (((value) >> 32) & 0xff))
#define MAKE_LMK05318_DPLL_REF_SYNC_PH_OFFSET_BY2(value) MAKE_LMK05318_REG_WR(DPLL_REF_SYNC_PH_OFFSET_BY2, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_DPLL_REF_SYNC_PH_OFFSET_BY3(value) MAKE_LMK05318_REG_WR(DPLL_REF_SYNC_PH_OFFSET_BY3, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_REF_SYNC_PH_OFFSET_BY4(value) MAKE_LMK05318_REG_WR(DPLL_REF_SYNC_PH_OFFSET_BY4, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_REF_SYNC_PH_OFFSET_BY5(value) MAKE_LMK05318_REG_WR(DPLL_REF_SYNC_PH_OFFSET_BY5, (((value) << 0) & 0xff))
// Register R346 [0x15a] -- DPLL_FDEV_CTL

enum dpll_fdev_ctl_fields_t {
    DPLL_FDEV_EN_OFF = 0x0,
    DPLL_FDEV_EN_MSK = 0x1,
};
#define MAKE_LMK05318_DPLL_FDEV_CTL(dpll_fdev_en) MAKE_LMK05318_REG_WR(DPLL_FDEV_CTL, \
    (((dpll_fdev_en) << DPLL_FDEV_EN_OFF) & DPLL_FDEV_EN_MSK))
// Register R347 [0x15b] -- DPLL_FDEV


#define MAKE_LMK05318_DPLL_FDEV_BY0(value) MAKE_LMK05318_REG_WR(DPLL_FDEV_BY0, (((value) >> 32) & 0xff))
#define MAKE_LMK05318_DPLL_FDEV_BY1(value) MAKE_LMK05318_REG_WR(DPLL_FDEV_BY1, (((value) >> 24) & 0xff))
#define MAKE_LMK05318_DPLL_FDEV_BY2(value) MAKE_LMK05318_REG_WR(DPLL_FDEV_BY2, (((value) >> 16) & 0xff))
#define MAKE_LMK05318_DPLL_FDEV_BY3(value) MAKE_LMK05318_REG_WR(DPLL_FDEV_BY3, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_DPLL_FDEV_BY4(value) MAKE_LMK05318_REG_WR(DPLL_FDEV_BY4, (((value) << 0) & 0xff))
// Register R352 [0x160] -- DPLL_FDEV_REG_CTL

enum dpll_fdev_reg_ctl_fields_t {
    DPLL_FDEV_REG_UPDATE_OFF = 0x0,
    DPLL_FDEV_REG_UPDATE_MSK = 0x1,
};
#define MAKE_LMK05318_DPLL_FDEV_REG_CTL(dpll_fdev_reg_update) MAKE_LMK05318_REG_WR(DPLL_FDEV_REG_CTL, \
    (((dpll_fdev_reg_update) << DPLL_FDEV_REG_UPDATE_OFF) & DPLL_FDEV_REG_UPDATE_MSK))
// Register R357 [0x165] -- PLL1_CALSTAT1

enum pll1_calstat1_fields_t {
    PLL1_VM_INSIDE_OFF = 0x5,
    PLL1_VM_INSIDE_MSK = 0x20,
};
#define MAKE_LMK05318_PLL1_CALSTAT1(pll1_vm_inside) MAKE_LMK05318_REG_WR(PLL1_CALSTAT1, \
    (((pll1_vm_inside) << PLL1_VM_INSIDE_OFF) & PLL1_VM_INSIDE_MSK))
// Register R367 [0x16f] -- PLL2_CALSTAT1

enum pll2_calstat1_fields_t {
    PLL2_VM_INSIDE_OFF = 0x5,
    PLL2_VM_INSIDE_MSK = 0x20,
};
#define MAKE_LMK05318_PLL2_CALSTAT1(pll2_vm_inside) MAKE_LMK05318_REG_WR(PLL2_CALSTAT1, \
    (((pll2_vm_inside) << PLL2_VM_INSIDE_OFF) & PLL2_VM_INSIDE_MSK))
// Register R411 [0x19b] -- REFVALSTAT

enum refvalstat_fields_t {
    SECREF_VALSTAT_OFF = 0x3,
    SECREF_VALSTAT_MSK = 0x8,
    PRIREF_VALSTAT_OFF = 0x2,
    PRIREF_VALSTAT_MSK = 0x4,
};
#define MAKE_LMK05318_REFVALSTAT(secref_valstat, priref_valstat) MAKE_LMK05318_REG_WR(REFVALSTAT, \
    (((secref_valstat) << SECREF_VALSTAT_OFF) & SECREF_VALSTAT_MSK) |  \
    (((priref_valstat) << PRIREF_VALSTAT_OFF) & PRIREF_VALSTAT_MSK))
// Register R156 [0x9c] -- NVMCNT

// Register R157 [0x9d] -- NVMCTL

// Register R159 [0x9f] -- MEMADR


#define MAKE_LMK05318_MEMADR_BY0(value) MAKE_LMK05318_REG_WR(MEMADR_BY0, (((value) >> 8) & 0xff))
#define MAKE_LMK05318_MEMADR_BY1(value) MAKE_LMK05318_REG_WR(MEMADR_BY1, (((value) << 0) & 0xff))
// Register R161 [0xa1] -- NVMDAT

// Register R162 [0xa2] -- RAMDAT

// Register R164 [0xa4] -- NVMUNLK

