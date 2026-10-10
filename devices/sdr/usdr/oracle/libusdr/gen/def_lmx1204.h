enum lmx1204_regs_t {
    R0 = 0x0,
    R2 = 0x2,
    R3 = 0x3,
    R4 = 0x4,
    R5 = 0x5,
    R6 = 0x6,
    R7 = 0x7,
    R8 = 0x8,
    R9 = 0x9,
    R11 = 0xb,
    R12 = 0xc,
    R13 = 0xd,
    R14 = 0xe,
    R15 = 0xf,
    R16 = 0x10,
    R17 = 0x11,
    R18 = 0x12,
    R19 = 0x13,
    R20 = 0x14,
    R21 = 0x15,
    R22 = 0x16,
    R23 = 0x17,
    R24 = 0x18,
    R25 = 0x19,
    R28 = 0x1c,
    R29 = 0x1d,
    R33 = 0x21,
    R34 = 0x22,
    R65 = 0x41,
    R67 = 0x43,
    R72 = 0x48,
    R75 = 0x4b,
    R79 = 0x4f,
    R86 = 0x56,
    R90 = 0x5a,
};
#define MAKE_LMX1204_REG_WR(a, v) (((a) << 16) | ((v) & 0xffff))
#define MAKE_LMX1204_REG_RD(a) (0x800000 | ((a) << 16))
// Register R0 [0x0] -- R0

enum r0_fields_t {
    R0_RESERVED_0_OFF = 0x3,
    R0_RESERVED_0_MSK = 0xfff8,
    POWERDOWN_OFF = 0x2,
    POWERDOWN_MSK = 0x4,
    R0_RESERVED_1_OFF = 0x1,
    R0_RESERVED_1_MSK = 0x2,
    RESET_OFF = 0x0,
    RESET_MSK = 0x1,
};
#define MAKE_LMX1204_R0(r0_reserved_0, powerdown, r0_reserved_1, reset) MAKE_LMX1204_REG_WR(R0, \
    (((r0_reserved_0) << R0_RESERVED_0_OFF) & R0_RESERVED_0_MSK) |  \
    (((powerdown) << POWERDOWN_OFF) & POWERDOWN_MSK) |  \
    (((r0_reserved_1) << R0_RESERVED_1_OFF) & R0_RESERVED_1_MSK) |  \
    (((reset) << RESET_OFF) & RESET_MSK))
// Register R2 [0x2] -- R2
enum smclk_div_pre_options {
    SMCLK_DIV_PRE_PL2 = 2,
    SMCLK_DIV_PRE_PL4 = 4,
    SMCLK_DIV_PRE_PL8 = 8,
};

enum r2_fields_t {
    R2_RESERVED_0_OFF = 0xb,
    R2_RESERVED_0_MSK = 0xf800,
    R2_RESERVED_1_OFF = 0xa,
    R2_RESERVED_1_MSK = 0x400,
    SMCLK_DIV_PRE_OFF = 0x6,
    SMCLK_DIV_PRE_MSK = 0x3c0,
    SMCLK_EN_OFF = 0x5,
    SMCLK_EN_MSK = 0x20,
    R2_RESERVED_2_OFF = 0x0,
    R2_RESERVED_2_MSK = 0x1f,
};
#define MAKE_LMX1204_R2(r2_reserved_0, r2_reserved_1, smclk_div_pre, smclk_en, r2_reserved_2) MAKE_LMX1204_REG_WR(R2, \
    (((r2_reserved_0) << R2_RESERVED_0_OFF) & R2_RESERVED_0_MSK) |  \
    (((r2_reserved_1) << R2_RESERVED_1_OFF) & R2_RESERVED_1_MSK) |  \
    (((smclk_div_pre) << SMCLK_DIV_PRE_OFF) & SMCLK_DIV_PRE_MSK) |  \
    (((smclk_en) << SMCLK_EN_OFF) & SMCLK_EN_MSK) |  \
    (((r2_reserved_2) << R2_RESERVED_2_OFF) & R2_RESERVED_2_MSK))
// Register R3 [0x3] -- R3
enum smclk_div_options {
    SMCLK_DIV_PL1 = 0,
    SMCLK_DIV_PL2 = 1,
    SMCLK_DIV_PL4 = 2,
    SMCLK_DIV_PL8 = 3,
    SMCLK_DIV_PL16 = 4,
    SMCLK_DIV_PL32 = 5,
    SMCLK_DIV_PL64 = 6,
    SMCLK_DIV_PL128 = 7,
};

enum r3_fields_t {
    CH3_EN_OFF = 0xf,
    CH3_EN_MSK = 0x8000,
    CH2_EN_OFF = 0xe,
    CH2_EN_MSK = 0x4000,
    CH1_EN_OFF = 0xd,
    CH1_EN_MSK = 0x2000,
    CH0_EN_OFF = 0xc,
    CH0_EN_MSK = 0x1000,
    LOGIC_MUTE_CAL_OFF = 0xb,
    LOGIC_MUTE_CAL_MSK = 0x800,
    CH3_MUTE_CAL_OFF = 0xa,
    CH3_MUTE_CAL_MSK = 0x400,
    CH2_MUTE_CAL_OFF = 0x9,
    CH2_MUTE_CAL_MSK = 0x200,
    CH1_MUTE_CAL_OFF = 0x8,
    CH1_MUTE_CAL_MSK = 0x100,
    CH0_MUTE_CAL_OFF = 0x7,
    CH0_MUTE_CAL_MSK = 0x80,
    R3_RESERVED_0_OFF = 0x3,
    R3_RESERVED_0_MSK = 0x78,
    SMCLK_DIV_OFF = 0x0,
    SMCLK_DIV_MSK = 0x7,
};
#define MAKE_LMX1204_R3(ch3_en, ch2_en, ch1_en, ch0_en, logic_mute_cal, ch3_mute_cal, ch2_mute_cal, ch1_mute_cal, ch0_mute_cal, r3_reserved_0, smclk_div) MAKE_LMX1204_REG_WR(R3, \
    (((ch3_en) << CH3_EN_OFF) & CH3_EN_MSK) |  \
    (((ch2_en) << CH2_EN_OFF) & CH2_EN_MSK) |  \
    (((ch1_en) << CH1_EN_OFF) & CH1_EN_MSK) |  \
    (((ch0_en) << CH0_EN_OFF) & CH0_EN_MSK) |  \
    (((logic_mute_cal) << LOGIC_MUTE_CAL_OFF) & LOGIC_MUTE_CAL_MSK) |  \
    (((ch3_mute_cal) << CH3_MUTE_CAL_OFF) & CH3_MUTE_CAL_MSK) |  \
    (((ch2_mute_cal) << CH2_MUTE_CAL_OFF) & CH2_MUTE_CAL_MSK) |  \
    (((ch1_mute_cal) << CH1_MUTE_CAL_OFF) & CH1_MUTE_CAL_MSK) |  \
    (((ch0_mute_cal) << CH0_MUTE_CAL_OFF) & CH0_MUTE_CAL_MSK) |  \
    (((r3_reserved_0) << R3_RESERVED_0_OFF) & R3_RESERVED_0_MSK) |  \
    (((smclk_div) << SMCLK_DIV_OFF) & SMCLK_DIV_MSK))
// Register R4 [0x4] -- R4

enum r4_fields_t {
    R4_RESERVED_0_OFF = 0xe,
    R4_RESERVED_0_MSK = 0xc000,
    CLKOUT1_PWR_OFF = 0xb,
    CLKOUT1_PWR_MSK = 0x3800,
    CLKOUT0_PWR_OFF = 0x8,
    CLKOUT0_PWR_MSK = 0x700,
    SYSREFOUT3_EN_OFF = 0x7,
    SYSREFOUT3_EN_MSK = 0x80,
    SYSREFOUT2_EN_OFF = 0x6,
    SYSREFOUT2_EN_MSK = 0x40,
    SYSREFOUT1_EN_OFF = 0x5,
    SYSREFOUT1_EN_MSK = 0x20,
    SYSREFOUT0_EN_OFF = 0x4,
    SYSREFOUT0_EN_MSK = 0x10,
    CLKOUT3_EN_OFF = 0x3,
    CLKOUT3_EN_MSK = 0x8,
    CLKOUT2_EN_OFF = 0x2,
    CLKOUT2_EN_MSK = 0x4,
    CLKOUT1_EN_OFF = 0x1,
    CLKOUT1_EN_MSK = 0x2,
    CLKOUT0_EN_OFF = 0x0,
    CLKOUT0_EN_MSK = 0x1,
};
#define MAKE_LMX1204_R4(r4_reserved_0, clkout1_pwr, clkout0_pwr, sysrefout3_en, sysrefout2_en, sysrefout1_en, sysrefout0_en, clkout3_en, clkout2_en, clkout1_en, clkout0_en) MAKE_LMX1204_REG_WR(R4, \
    (((r4_reserved_0) << R4_RESERVED_0_OFF) & R4_RESERVED_0_MSK) |  \
    (((clkout1_pwr) << CLKOUT1_PWR_OFF) & CLKOUT1_PWR_MSK) |  \
    (((clkout0_pwr) << CLKOUT0_PWR_OFF) & CLKOUT0_PWR_MSK) |  \
    (((sysrefout3_en) << SYSREFOUT3_EN_OFF) & SYSREFOUT3_EN_MSK) |  \
    (((sysrefout2_en) << SYSREFOUT2_EN_OFF) & SYSREFOUT2_EN_MSK) |  \
    (((sysrefout1_en) << SYSREFOUT1_EN_OFF) & SYSREFOUT1_EN_MSK) |  \
    (((sysrefout0_en) << SYSREFOUT0_EN_OFF) & SYSREFOUT0_EN_MSK) |  \
    (((clkout3_en) << CLKOUT3_EN_OFF) & CLKOUT3_EN_MSK) |  \
    (((clkout2_en) << CLKOUT2_EN_OFF) & CLKOUT2_EN_MSK) |  \
    (((clkout1_en) << CLKOUT1_EN_OFF) & CLKOUT1_EN_MSK) |  \
    (((clkout0_en) << CLKOUT0_EN_OFF) & CLKOUT0_EN_MSK))
// Register R5 [0x5] -- R5

enum r5_fields_t {
    R5_RESERVED_0_OFF = 0xf,
    R5_RESERVED_0_MSK = 0x8000,
    SYSREFOUT2_PWR_OFF = 0xc,
    SYSREFOUT2_PWR_MSK = 0x7000,
    SYSREFOUT1_PWR_OFF = 0x9,
    SYSREFOUT1_PWR_MSK = 0xe00,
    SYSREFOUT0_PWR_OFF = 0x6,
    SYSREFOUT0_PWR_MSK = 0x1c0,
    CLKOUT3_PWR_OFF = 0x3,
    CLKOUT3_PWR_MSK = 0x38,
    CLKOUT2_PWR_OFF = 0x0,
    CLKOUT2_PWR_MSK = 0x7,
};
#define MAKE_LMX1204_R5(r5_reserved_0, sysrefout2_pwr, sysrefout1_pwr, sysrefout0_pwr, clkout3_pwr, clkout2_pwr) MAKE_LMX1204_REG_WR(R5, \
    (((r5_reserved_0) << R5_RESERVED_0_OFF) & R5_RESERVED_0_MSK) |  \
    (((sysrefout2_pwr) << SYSREFOUT2_PWR_OFF) & SYSREFOUT2_PWR_MSK) |  \
    (((sysrefout1_pwr) << SYSREFOUT1_PWR_OFF) & SYSREFOUT1_PWR_MSK) |  \
    (((sysrefout0_pwr) << SYSREFOUT0_PWR_OFF) & SYSREFOUT0_PWR_MSK) |  \
    (((clkout3_pwr) << CLKOUT3_PWR_OFF) & CLKOUT3_PWR_MSK) |  \
    (((clkout2_pwr) << CLKOUT2_PWR_OFF) & CLKOUT2_PWR_MSK))
// Register R6 [0x6] -- R6

enum r6_fields_t {
    LOGICLKOUT_EN_OFF = 0xf,
    LOGICLKOUT_EN_MSK = 0x8000,
    SYSREFOUT3_VCM_OFF = 0xc,
    SYSREFOUT3_VCM_MSK = 0x7000,
    SYSREFOUT2_VCM_OFF = 0x9,
    SYSREFOUT2_VCM_MSK = 0xe00,
    SYSREFOUT1_VCM_OFF = 0x6,
    SYSREFOUT1_VCM_MSK = 0x1c0,
    SYSREFOUT0_VCM_OFF = 0x3,
    SYSREFOUT0_VCM_MSK = 0x38,
    SYSREFOUT3_PWR_OFF = 0x0,
    SYSREFOUT3_PWR_MSK = 0x7,
};
#define MAKE_LMX1204_R6(logiclkout_en, sysrefout3_vcm, sysrefout2_vcm, sysrefout1_vcm, sysrefout0_vcm, sysrefout3_pwr) MAKE_LMX1204_REG_WR(R6, \
    (((logiclkout_en) << LOGICLKOUT_EN_OFF) & LOGICLKOUT_EN_MSK) |  \
    (((sysrefout3_vcm) << SYSREFOUT3_VCM_OFF) & SYSREFOUT3_VCM_MSK) |  \
    (((sysrefout2_vcm) << SYSREFOUT2_VCM_OFF) & SYSREFOUT2_VCM_MSK) |  \
    (((sysrefout1_vcm) << SYSREFOUT1_VCM_OFF) & SYSREFOUT1_VCM_MSK) |  \
    (((sysrefout0_vcm) << SYSREFOUT0_VCM_OFF) & SYSREFOUT0_VCM_MSK) |  \
    (((sysrefout3_pwr) << SYSREFOUT3_PWR_OFF) & SYSREFOUT3_PWR_MSK))
// Register R7 [0x7] -- R7
enum logisysrefout_vcm_options {
    LOGISYSREFOUT_VCM_0X0_EQ_1_2_V = 0,
    LOGISYSREFOUT_VCM_0X1_EQ_1_1_V = 1,
    LOGISYSREFOUT_VCM_0X2_EQ_1_0_V = 2,
    LOGISYSREFOUT_VCM_0X3_EQ_0_9_V = 3,
};
enum logiclkout_vcm_options {
    LOGICLKOUT_VCM_0X0_EQ_1_2_V = 0,
    LOGICLKOUT_VCM_0X1_EQ_1_1_V = 1,
    LOGICLKOUT_VCM_0X2_EQ_1_0_V = 2,
    LOGICLKOUT_VCM_0X3_EQ_0_9_V = 3,
};

enum r7_fields_t {
    R7_RESERVED_0_OFF = 0xf,
    R7_RESERVED_0_MSK = 0x8000,
    LOGISYSREFOUT_VCM_OFF = 0xd,
    LOGISYSREFOUT_VCM_MSK = 0x6000,
    LOGICLKOUT_VCM_OFF = 0xb,
    LOGICLKOUT_VCM_MSK = 0x1800,
    LOGISYSREFOUT_PREDRV_PWR_OFF = 0x9,
    LOGISYSREFOUT_PREDRV_PWR_MSK = 0x600,
    LOGICLKOUT_PREDRV_PWR_OFF = 0x7,
    LOGICLKOUT_PREDRV_PWR_MSK = 0x180,
    LOGISYSREFOUT_PWR_OFF = 0x4,
    LOGISYSREFOUT_PWR_MSK = 0x70,
    LOGICLKOUT_PWR_OFF = 0x1,
    LOGICLKOUT_PWR_MSK = 0xe,
    LOGISYSREFOUT_EN_OFF = 0x0,
    LOGISYSREFOUT_EN_MSK = 0x1,
};
#define MAKE_LMX1204_R7(r7_reserved_0, logisysrefout_vcm, logiclkout_vcm, logisysrefout_predrv_pwr, logiclkout_predrv_pwr, logisysrefout_pwr, logiclkout_pwr, logisysrefout_en) MAKE_LMX1204_REG_WR(R7, \
    (((r7_reserved_0) << R7_RESERVED_0_OFF) & R7_RESERVED_0_MSK) |  \
    (((logisysrefout_vcm) << LOGISYSREFOUT_VCM_OFF) & LOGISYSREFOUT_VCM_MSK) |  \
    (((logiclkout_vcm) << LOGICLKOUT_VCM_OFF) & LOGICLKOUT_VCM_MSK) |  \
    (((logisysrefout_predrv_pwr) << LOGISYSREFOUT_PREDRV_PWR_OFF) & LOGISYSREFOUT_PREDRV_PWR_MSK) |  \
    (((logiclkout_predrv_pwr) << LOGICLKOUT_PREDRV_PWR_OFF) & LOGICLKOUT_PREDRV_PWR_MSK) |  \
    (((logisysrefout_pwr) << LOGISYSREFOUT_PWR_OFF) & LOGISYSREFOUT_PWR_MSK) |  \
    (((logiclkout_pwr) << LOGICLKOUT_PWR_OFF) & LOGICLKOUT_PWR_MSK) |  \
    (((logisysrefout_en) << LOGISYSREFOUT_EN_OFF) & LOGISYSREFOUT_EN_MSK))
// Register R8 [0x8] -- R8
enum logiclk_div_pre_options {
    LOGICLK_DIV_PRE_PL1 = 1,
    LOGICLK_DIV_PRE_PL2 = 2,
    LOGICLK_DIV_PRE_PL4 = 4,
};
enum logisysrefout_fmt_options {
    LOGISYSREFOUT_FMT_LVDS = 0,
    LOGISYSREFOUT_FMT_LVPECL = 1,
    LOGISYSREFOUT_FMT_CML = 2,
};
enum logiclkout_fmt_options {
    LOGICLKOUT_FMT_LVDS = 0,
    LOGICLKOUT_FMT_LVPECL = 1,
    LOGICLKOUT_FMT_CML = 2,
};

enum r8_fields_t {
    R8_RESERVED_0_OFF = 0x9,
    R8_RESERVED_0_MSK = 0xfe00,
    LOGICLK_DIV_PRE_OFF = 0x6,
    LOGICLK_DIV_PRE_MSK = 0x1c0,
    R8_RESERVED_1_OFF = 0x5,
    R8_RESERVED_1_MSK = 0x20,
    LOGIC_EN_OFF = 0x4,
    LOGIC_EN_MSK = 0x10,
    LOGISYSREFOUT_FMT_OFF = 0x2,
    LOGISYSREFOUT_FMT_MSK = 0xc,
    LOGICLKOUT_FMT_OFF = 0x0,
    LOGICLKOUT_FMT_MSK = 0x3,
};
#define MAKE_LMX1204_R8(r8_reserved_0, logiclk_div_pre, r8_reserved_1, logic_en, logisysrefout_fmt, logiclkout_fmt) MAKE_LMX1204_REG_WR(R8, \
    (((r8_reserved_0) << R8_RESERVED_0_OFF) & R8_RESERVED_0_MSK) |  \
    (((logiclk_div_pre) << LOGICLK_DIV_PRE_OFF) & LOGICLK_DIV_PRE_MSK) |  \
    (((r8_reserved_1) << R8_RESERVED_1_OFF) & R8_RESERVED_1_MSK) |  \
    (((logic_en) << LOGIC_EN_OFF) & LOGIC_EN_MSK) |  \
    (((logisysrefout_fmt) << LOGISYSREFOUT_FMT_OFF) & LOGISYSREFOUT_FMT_MSK) |  \
    (((logiclkout_fmt) << LOGICLKOUT_FMT_OFF) & LOGICLKOUT_FMT_MSK))
// Register R9 [0x9] -- R9
enum sysrefreq_vcm_options {
    SYSREFREQ_VCM_1_3_V = 0,
    SYSREFREQ_VCM_1_1_V = 1,
    SYSREFREQ_VCM_1_5_V = 2,
    SYSREFREQ_VCM_DISABLED_DC_COUPLED_ONLY = 3,
};

enum r9_fields_t {
    SYSREFREQ_VCM_OFF = 0xe,
    SYSREFREQ_VCM_MSK = 0xc000,
    SYNC_EN_OFF = 0xd,
    SYNC_EN_MSK = 0x2000,
    LOGICLK_DIV_PD_OFF = 0xc,
    LOGICLK_DIV_PD_MSK = 0x1000,
    LOGICLK_DIV_BYPASS_OFF = 0xb,
    LOGICLK_DIV_BYPASS_MSK = 0x800,
    R9_RESERVED_0_OFF = 0xa,
    R9_RESERVED_0_MSK = 0x400,
    LOGICLK_DIV_OFF = 0x0,
    LOGICLK_DIV_MSK = 0x3ff,
};
#define MAKE_LMX1204_R9(sysrefreq_vcm, sync_en, logiclk_div_pd, logiclk_div_bypass, r9_reserved_0, logiclk_div) MAKE_LMX1204_REG_WR(R9, \
    (((sysrefreq_vcm) << SYSREFREQ_VCM_OFF) & SYSREFREQ_VCM_MSK) |  \
    (((sync_en) << SYNC_EN_OFF) & SYNC_EN_MSK) |  \
    (((logiclk_div_pd) << LOGICLK_DIV_PD_OFF) & LOGICLK_DIV_PD_MSK) |  \
    (((logiclk_div_bypass) << LOGICLK_DIV_BYPASS_OFF) & LOGICLK_DIV_BYPASS_MSK) |  \
    (((r9_reserved_0) << R9_RESERVED_0_OFF) & R9_RESERVED_0_MSK) |  \
    (((logiclk_div) << LOGICLK_DIV_OFF) & LOGICLK_DIV_MSK))
// Register R11 [0xb] -- R11

enum r11_fields_t {
    RB_CLKPOS_L_OFF = 0x0,
    RB_CLKPOS_L_MSK = 0xffff,
};
#define MAKE_LMX1204_R11(rb_clkpos_l) MAKE_LMX1204_REG_WR(R11, \
    (((rb_clkpos_l) << RB_CLKPOS_L_OFF) & RB_CLKPOS_L_MSK))
// Register R12 [0xc] -- R12

enum r12_fields_t {
    RB_CLKPOS_U_OFF = 0x0,
    RB_CLKPOS_U_MSK = 0xffff,
};
#define MAKE_LMX1204_R12(rb_clkpos_u) MAKE_LMX1204_REG_WR(R12, \
    (((rb_clkpos_u) << RB_CLKPOS_U_OFF) & RB_CLKPOS_U_MSK))
// Register R13 [0xd] -- R13
enum sysrefreq_delay_stepsize_options {
    SYSREFREQ_DELAY_STEPSIZE_28_PS_1_4_GHZ_TO_2_7_GHZ = 0,
    SYSREFREQ_DELAY_STEPSIZE_15_PS_2_4_GHZ_TO_4_7_GHZ = 1,
    SYSREFREQ_DELAY_STEPSIZE_11_PS_3_1_GHZ_TO_5_7_GHZ = 2,
    SYSREFREQ_DELAY_STEPSIZE_8_PS_4_5_GHZ_TO_12_8_GHZ = 3,
};

enum r13_fields_t {
    R13_RESERVED_0_OFF = 0x2,
    R13_RESERVED_0_MSK = 0xfffc,
    SYSREFREQ_DELAY_STEPSIZE_OFF = 0x0,
    SYSREFREQ_DELAY_STEPSIZE_MSK = 0x3,
};
#define MAKE_LMX1204_R13(r13_reserved_0, sysrefreq_delay_stepsize) MAKE_LMX1204_REG_WR(R13, \
    (((r13_reserved_0) << R13_RESERVED_0_OFF) & R13_RESERVED_0_MSK) |  \
    (((sysrefreq_delay_stepsize) << SYSREFREQ_DELAY_STEPSIZE_OFF) & SYSREFREQ_DELAY_STEPSIZE_MSK))
// Register R14 [0xe] -- R14
enum sysrefreq_mode_options {
    SYSREFREQ_MODE_SYNC_PIN = 0,
    SYSREFREQ_MODE_SYSREFREQ_PIN = 1,
};

enum r14_fields_t {
    R14_RESERVED_0_OFF = 0x9,
    R14_RESERVED_0_MSK = 0xfe00,
    SYNC_MUTE_PD_OFF = 0x8,
    SYNC_MUTE_PD_MSK = 0x100,
    R14_RESERVED_1_OFF = 0x3,
    R14_RESERVED_1_MSK = 0xf8,
    CLKPOS_CAPTURE_EN_OFF = 0x2,
    CLKPOS_CAPTURE_EN_MSK = 0x4,
    SYSREFREQ_MODE_OFF = 0x1,
    SYSREFREQ_MODE_MSK = 0x2,
    SYSREFREQ_LATCH_OFF = 0x0,
    SYSREFREQ_LATCH_MSK = 0x1,
};
#define MAKE_LMX1204_R14(r14_reserved_0, sync_mute_pd, r14_reserved_1, clkpos_capture_en, sysrefreq_mode, sysrefreq_latch) MAKE_LMX1204_REG_WR(R14, \
    (((r14_reserved_0) << R14_RESERVED_0_OFF) & R14_RESERVED_0_MSK) |  \
    (((sync_mute_pd) << SYNC_MUTE_PD_OFF) & SYNC_MUTE_PD_MSK) |  \
    (((r14_reserved_1) << R14_RESERVED_1_OFF) & R14_RESERVED_1_MSK) |  \
    (((clkpos_capture_en) << CLKPOS_CAPTURE_EN_OFF) & CLKPOS_CAPTURE_EN_MSK) |  \
    (((sysrefreq_mode) << SYSREFREQ_MODE_OFF) & SYSREFREQ_MODE_MSK) |  \
    (((sysrefreq_latch) << SYSREFREQ_LATCH_OFF) & SYSREFREQ_LATCH_MSK))
// Register R15 [0xf] -- R15
enum sysref_div_pre_options {
    SYSREF_DIV_PRE_PL1 = 0,
    SYSREF_DIV_PRE_PL2 = 1,
    SYSREF_DIV_PRE_PL4 = 2,
};

enum r15_fields_t {
    R15_RESERVED_0_OFF = 0xc,
    R15_RESERVED_0_MSK = 0xf000,
    SYSREF_DIV_PRE_OFF = 0xa,
    SYSREF_DIV_PRE_MSK = 0xc00,
    R15_RESERVED_1_OFF = 0x8,
    R15_RESERVED_1_MSK = 0x300,
    SYSREF_EN_OFF = 0x7,
    SYSREF_EN_MSK = 0x80,
    SYSREFREQ_DELAY_STEP_OFF = 0x1,
    SYSREFREQ_DELAY_STEP_MSK = 0x7e,
    SYSREFREQ_CLR_OFF = 0x0,
    SYSREFREQ_CLR_MSK = 0x1,
};
#define MAKE_LMX1204_R15(r15_reserved_0, sysref_div_pre, r15_reserved_1, sysref_en, sysrefreq_delay_step, sysrefreq_clr) MAKE_LMX1204_REG_WR(R15, \
    (((r15_reserved_0) << R15_RESERVED_0_OFF) & R15_RESERVED_0_MSK) |  \
    (((sysref_div_pre) << SYSREF_DIV_PRE_OFF) & SYSREF_DIV_PRE_MSK) |  \
    (((r15_reserved_1) << R15_RESERVED_1_OFF) & R15_RESERVED_1_MSK) |  \
    (((sysref_en) << SYSREF_EN_OFF) & SYSREF_EN_MSK) |  \
    (((sysrefreq_delay_step) << SYSREFREQ_DELAY_STEP_OFF) & SYSREFREQ_DELAY_STEP_MSK) |  \
    (((sysrefreq_clr) << SYSREFREQ_CLR_OFF) & SYSREFREQ_CLR_MSK))
// Register R16 [0x10] -- R16

enum r16_fields_t {
    SYSREF_PULSE_COUNT_OFF = 0xc,
    SYSREF_PULSE_COUNT_MSK = 0xf000,
    SYSREF_DIV_OFF = 0x0,
    SYSREF_DIV_MSK = 0xfff,
};
#define MAKE_LMX1204_R16(sysref_pulse_count, sysref_div) MAKE_LMX1204_REG_WR(R16, \
    (((sysref_pulse_count) << SYSREF_PULSE_COUNT_OFF) & SYSREF_PULSE_COUNT_MSK) |  \
    (((sysref_div) << SYSREF_DIV_OFF) & SYSREF_DIV_MSK))
// Register R17 [0x11] -- R17
enum sysrefout0_delay_phase_options {
    SYSREFOUT0_DELAY_PHASE_ICLK0 = 0,
    SYSREFOUT0_DELAY_PHASE_QCLK0 = 1,
    SYSREFOUT0_DELAY_PHASE_QCLK1 = 2,
    SYSREFOUT0_DELAY_PHASE_ICLK1 = 3,
};
enum sysref_mode_options {
    SYSREF_MODE_CONTINUOUS_GENERATOR_MODE = 0,
    SYSREF_MODE_PULSER_GENERATOR_MODE = 1,
    SYSREF_MODE_REPEATER_REPEATER_MODE = 2,
};

enum r17_fields_t {
    R17_RESERVED_0_OFF = 0xb,
    R17_RESERVED_0_MSK = 0xf800,
    SYSREFOUT0_DELAY_I_OFF = 0x4,
    SYSREFOUT0_DELAY_I_MSK = 0x7f0,
    SYSREFOUT0_DELAY_PHASE_OFF = 0x2,
    SYSREFOUT0_DELAY_PHASE_MSK = 0xc,
    SYSREF_MODE_OFF = 0x0,
    SYSREF_MODE_MSK = 0x3,
};
#define MAKE_LMX1204_R17(r17_reserved_0, sysrefout0_delay_i, sysrefout0_delay_phase, sysref_mode) MAKE_LMX1204_REG_WR(R17, \
    (((r17_reserved_0) << R17_RESERVED_0_OFF) & R17_RESERVED_0_MSK) |  \
    (((sysrefout0_delay_i) << SYSREFOUT0_DELAY_I_OFF) & SYSREFOUT0_DELAY_I_MSK) |  \
    (((sysrefout0_delay_phase) << SYSREFOUT0_DELAY_PHASE_OFF) & SYSREFOUT0_DELAY_PHASE_MSK) |  \
    (((sysref_mode) << SYSREF_MODE_OFF) & SYSREF_MODE_MSK))
// Register R18 [0x12] -- R18
enum sysrefout1_delay_phase_options {
    SYSREFOUT1_DELAY_PHASE_ICLK0 = 0,
    SYSREFOUT1_DELAY_PHASE_QCLK0 = 1,
    SYSREFOUT1_DELAY_PHASE_QCLK1 = 2,
    SYSREFOUT1_DELAY_PHASE_ICLK1 = 3,
};

enum r18_fields_t {
    SYSREFOUT1_DELAY_I_OFF = 0x9,
    SYSREFOUT1_DELAY_I_MSK = 0xfe00,
    SYSREFOUT1_DELAY_PHASE_OFF = 0x7,
    SYSREFOUT1_DELAY_PHASE_MSK = 0x180,
    SYSREFOUT0_DELAY_Q_OFF = 0x0,
    SYSREFOUT0_DELAY_Q_MSK = 0x7f,
};
#define MAKE_LMX1204_R18(sysrefout1_delay_i, sysrefout1_delay_phase, sysrefout0_delay_q) MAKE_LMX1204_REG_WR(R18, \
    (((sysrefout1_delay_i) << SYSREFOUT1_DELAY_I_OFF) & SYSREFOUT1_DELAY_I_MSK) |  \
    (((sysrefout1_delay_phase) << SYSREFOUT1_DELAY_PHASE_OFF) & SYSREFOUT1_DELAY_PHASE_MSK) |  \
    (((sysrefout0_delay_q) << SYSREFOUT0_DELAY_Q_OFF) & SYSREFOUT0_DELAY_Q_MSK))
// Register R19 [0x13] -- R19
enum sysrefout2_delay_phase_options {
    SYSREFOUT2_DELAY_PHASE_ICLK0 = 0,
    SYSREFOUT2_DELAY_PHASE_QCLK0 = 1,
    SYSREFOUT2_DELAY_PHASE_QCLK1 = 2,
    SYSREFOUT2_DELAY_PHASE_ICLK1 = 3,
};

enum r19_fields_t {
    SYSREFOUT2_DELAY_I_OFF = 0x9,
    SYSREFOUT2_DELAY_I_MSK = 0xfe00,
    SYSREFOUT2_DELAY_PHASE_OFF = 0x7,
    SYSREFOUT2_DELAY_PHASE_MSK = 0x180,
    SYSREFOUT1_DELAY_Q_OFF = 0x0,
    SYSREFOUT1_DELAY_Q_MSK = 0x7f,
};
#define MAKE_LMX1204_R19(sysrefout2_delay_i, sysrefout2_delay_phase, sysrefout1_delay_q) MAKE_LMX1204_REG_WR(R19, \
    (((sysrefout2_delay_i) << SYSREFOUT2_DELAY_I_OFF) & SYSREFOUT2_DELAY_I_MSK) |  \
    (((sysrefout2_delay_phase) << SYSREFOUT2_DELAY_PHASE_OFF) & SYSREFOUT2_DELAY_PHASE_MSK) |  \
    (((sysrefout1_delay_q) << SYSREFOUT1_DELAY_Q_OFF) & SYSREFOUT1_DELAY_Q_MSK))
// Register R20 [0x14] -- R20
enum sysrefout3_delay_phase_options {
    SYSREFOUT3_DELAY_PHASE_ICLK0 = 0,
    SYSREFOUT3_DELAY_PHASE_QCLK0 = 1,
    SYSREFOUT3_DELAY_PHASE_QCLK1 = 2,
    SYSREFOUT3_DELAY_PHASE_ICLK1 = 3,
};

enum r20_fields_t {
    SYSREFOUT3_DELAY_I_OFF = 0x9,
    SYSREFOUT3_DELAY_I_MSK = 0xfe00,
    SYSREFOUT3_DELAY_PHASE_OFF = 0x7,
    SYSREFOUT3_DELAY_PHASE_MSK = 0x180,
    SYSREFOUT2_DELAY_Q_OFF = 0x0,
    SYSREFOUT2_DELAY_Q_MSK = 0x7f,
};
#define MAKE_LMX1204_R20(sysrefout3_delay_i, sysrefout3_delay_phase, sysrefout2_delay_q) MAKE_LMX1204_REG_WR(R20, \
    (((sysrefout3_delay_i) << SYSREFOUT3_DELAY_I_OFF) & SYSREFOUT3_DELAY_I_MSK) |  \
    (((sysrefout3_delay_phase) << SYSREFOUT3_DELAY_PHASE_OFF) & SYSREFOUT3_DELAY_PHASE_MSK) |  \
    (((sysrefout2_delay_q) << SYSREFOUT2_DELAY_Q_OFF) & SYSREFOUT2_DELAY_Q_MSK))
// Register R21 [0x15] -- R21
enum logisysrefout_delay_phase_options {
    LOGISYSREFOUT_DELAY_PHASE_ICLK0 = 0,
    LOGISYSREFOUT_DELAY_PHASE_QCLK0 = 1,
    LOGISYSREFOUT_DELAY_PHASE_QCLK1 = 2,
    LOGISYSREFOUT_DELAY_PHASE_ICLK1 = 3,
};

enum r21_fields_t {
    LOGISYSREFOUT_DELAY_I_OFF = 0x9,
    LOGISYSREFOUT_DELAY_I_MSK = 0xfe00,
    LOGISYSREFOUT_DELAY_PHASE_OFF = 0x7,
    LOGISYSREFOUT_DELAY_PHASE_MSK = 0x180,
    SYSREFOUT3_DELAY_Q_OFF = 0x0,
    SYSREFOUT3_DELAY_Q_MSK = 0x7f,
};
#define MAKE_LMX1204_R21(logisysrefout_delay_i, logisysrefout_delay_phase, sysrefout3_delay_q) MAKE_LMX1204_REG_WR(R21, \
    (((logisysrefout_delay_i) << LOGISYSREFOUT_DELAY_I_OFF) & LOGISYSREFOUT_DELAY_I_MSK) |  \
    (((logisysrefout_delay_phase) << LOGISYSREFOUT_DELAY_PHASE_OFF) & LOGISYSREFOUT_DELAY_PHASE_MSK) |  \
    (((sysrefout3_delay_q) << SYSREFOUT3_DELAY_Q_OFF) & SYSREFOUT3_DELAY_Q_MSK))
// Register R22 [0x16] -- R22
enum sysrefout1_delay_scale_options {
    SYSREFOUT1_DELAY_SCALE_400_MHZ_TO_800_MHZ = 0,
    SYSREFOUT1_DELAY_SCALE_200_MHZ_TO_400_MHZ = 1,
    SYSREFOUT1_DELAY_SCALE_150_MHZ_TO_200_MHZ = 2,
};
enum sysrefout0_delay_scale_options {
    SYSREFOUT0_DELAY_SCALE_400_MHZ_TO_800_MHZ = 0,
    SYSREFOUT0_DELAY_SCALE_200_MHZ_TO_400_MHZ = 1,
    SYSREFOUT0_DELAY_SCALE_150_MHZ_TO_200_MHZ = 2,
};
enum sysref_delay_div_options {
    SYSREF_DELAY_DIV_PL2_LE_1_6_GHZ = 0,
    SYSREF_DELAY_DIV_PL4_1_6_GHZ_TO_3_2_GHZ = 1,
    SYSREF_DELAY_DIV_PL8_3_2_GHZ_TO_6_4_GHZ = 2,
    SYSREF_DELAY_DIV_PL16_6_4_GHZ_TO_12_8_GHZ = 4,
};

enum r22_fields_t {
    SYSREFOUT1_DELAY_SCALE_OFF = 0xe,
    SYSREFOUT1_DELAY_SCALE_MSK = 0xc000,
    SYSREFOUT0_DELAY_SCALE_OFF = 0xc,
    SYSREFOUT0_DELAY_SCALE_MSK = 0x3000,
    SYSREF_DELAY_DIV_OFF = 0x9,
    SYSREF_DELAY_DIV_MSK = 0xe00,
    R22_RESERVED_0_OFF = 0x7,
    R22_RESERVED_0_MSK = 0x180,
    LOGISYSREFOUT_DELAY_Q_OFF = 0x0,
    LOGISYSREFOUT_DELAY_Q_MSK = 0x7f,
};
#define MAKE_LMX1204_R22(sysrefout1_delay_scale, sysrefout0_delay_scale, sysref_delay_div, r22_reserved_0, logisysrefout_delay_q) MAKE_LMX1204_REG_WR(R22, \
    (((sysrefout1_delay_scale) << SYSREFOUT1_DELAY_SCALE_OFF) & SYSREFOUT1_DELAY_SCALE_MSK) |  \
    (((sysrefout0_delay_scale) << SYSREFOUT0_DELAY_SCALE_OFF) & SYSREFOUT0_DELAY_SCALE_MSK) |  \
    (((sysref_delay_div) << SYSREF_DELAY_DIV_OFF) & SYSREF_DELAY_DIV_MSK) |  \
    (((r22_reserved_0) << R22_RESERVED_0_OFF) & R22_RESERVED_0_MSK) |  \
    (((logisysrefout_delay_q) << LOGISYSREFOUT_DELAY_Q_OFF) & LOGISYSREFOUT_DELAY_Q_MSK))
// Register R23 [0x17] -- R23
enum muxout_en_options {
    MUXOUT_EN_TRI_STATE = 0,
    MUXOUT_EN_PUSH_PULL = 1,
};
enum muxout_sel_options {
    MUXOUT_SEL_LOCK_DETECT_MULTIPLIER_ONLY = 0,
    MUXOUT_SEL_SDO_SPI_READBACK = 1,
};
enum logisysrefout_delay_scale_options {
    LOGISYSREFOUT_DELAY_SCALE_400_MHZ_TO_800_MHZ = 0,
    LOGISYSREFOUT_DELAY_SCALE_200_MHZ_TO_400_MHZ = 1,
    LOGISYSREFOUT_DELAY_SCALE_150_MHZ_TO_200_MHZ = 2,
};
enum sysrefout3_delay_scale_options {
    SYSREFOUT3_DELAY_SCALE_400_MHZ_TO_800_MHZ = 0,
    SYSREFOUT3_DELAY_SCALE_200_MHZ_TO_400_MHZ = 1,
    SYSREFOUT3_DELAY_SCALE_150_MHZ_TO_200_MHZ = 2,
};
enum sysrefout2_delay_scale_options {
    SYSREFOUT2_DELAY_SCALE_400_MHZ_TO_800_MHZ = 0,
    SYSREFOUT2_DELAY_SCALE_200_MHZ_TO_400_MHZ = 1,
    SYSREFOUT2_DELAY_SCALE_150_MHZ_TO_200_MHZ = 2,
};

enum r23_fields_t {
    EN_TEMPSENSE_OFF = 0xf,
    EN_TEMPSENSE_MSK = 0x8000,
    R23_RESERVED_0_OFF = 0xe,
    R23_RESERVED_0_MSK = 0x4000,
    MUXOUT_EN_OFF = 0xd,
    MUXOUT_EN_MSK = 0x2000,
    R23_RESERVED_1_OFF = 0x7,
    R23_RESERVED_1_MSK = 0x1f80,
    MUXOUT_SEL_OFF = 0x6,
    MUXOUT_SEL_MSK = 0x40,
    LOGISYSREFOUT_DELAY_SCALE_OFF = 0x4,
    LOGISYSREFOUT_DELAY_SCALE_MSK = 0x30,
    SYSREFOUT3_DELAY_SCALE_OFF = 0x2,
    SYSREFOUT3_DELAY_SCALE_MSK = 0xc,
    SYSREFOUT2_DELAY_SCALE_OFF = 0x0,
    SYSREFOUT2_DELAY_SCALE_MSK = 0x3,
};
#define MAKE_LMX1204_R23(en_tempsense, r23_reserved_0, muxout_en, r23_reserved_1, muxout_sel, logisysrefout_delay_scale, sysrefout3_delay_scale, sysrefout2_delay_scale) MAKE_LMX1204_REG_WR(R23, \
    (((en_tempsense) << EN_TEMPSENSE_OFF) & EN_TEMPSENSE_MSK) |  \
    (((r23_reserved_0) << R23_RESERVED_0_OFF) & R23_RESERVED_0_MSK) |  \
    (((muxout_en) << MUXOUT_EN_OFF) & MUXOUT_EN_MSK) |  \
    (((r23_reserved_1) << R23_RESERVED_1_OFF) & R23_RESERVED_1_MSK) |  \
    (((muxout_sel) << MUXOUT_SEL_OFF) & MUXOUT_SEL_MSK) |  \
    (((logisysrefout_delay_scale) << LOGISYSREFOUT_DELAY_SCALE_OFF) & LOGISYSREFOUT_DELAY_SCALE_MSK) |  \
    (((sysrefout3_delay_scale) << SYSREFOUT3_DELAY_SCALE_OFF) & SYSREFOUT3_DELAY_SCALE_MSK) |  \
    (((sysrefout2_delay_scale) << SYSREFOUT2_DELAY_SCALE_OFF) & SYSREFOUT2_DELAY_SCALE_MSK))
// Register R24 [0x18] -- R24

enum r24_fields_t {
    R24_RESERVED_0_OFF = 0xe,
    R24_RESERVED_0_MSK = 0xc000,
    R24_RESERVED_1_OFF = 0xc,
    R24_RESERVED_1_MSK = 0x3000,
    RB_TEMPSENSE_OFF = 0x1,
    RB_TEMPSENSE_MSK = 0xffe,
    EN_TS_COUNT_OFF = 0x0,
    EN_TS_COUNT_MSK = 0x1,
};
#define MAKE_LMX1204_R24(r24_reserved_0, r24_reserved_1, rb_tempsense, en_ts_count) MAKE_LMX1204_REG_WR(R24, \
    (((r24_reserved_0) << R24_RESERVED_0_OFF) & R24_RESERVED_0_MSK) |  \
    (((r24_reserved_1) << R24_RESERVED_1_OFF) & R24_RESERVED_1_MSK) |  \
    (((rb_tempsense) << RB_TEMPSENSE_OFF) & RB_TEMPSENSE_MSK) |  \
    (((en_ts_count) << EN_TS_COUNT_OFF) & EN_TS_COUNT_MSK))
// Register R25 [0x19] -- R25
enum clk_mux_options {
    CLK_MUX_BUFFER_MODE = 1,
    CLK_MUX_DIVIDER_MODE = 2,
    CLK_MUX_MULTIPLIER_MODE = 3,
};

enum r25_fields_t {
    R25_RESERVED_0_OFF = 0x7,
    R25_RESERVED_0_MSK = 0xff80,
    CLK_DIV_RST_OFF = 0x6,
    CLK_DIV_RST_MSK = 0x40,
    CLK_DIVCLK_MULT_OFF = 0x3,
    CLK_DIVCLK_MULT_MSK = 0x38,
    CLK_MUX_OFF = 0x0,
    CLK_MUX_MSK = 0x7,
};
#define MAKE_LMX1204_R25(r25_reserved_0, clk_div_rst, clk_divclk_mult, clk_mux) MAKE_LMX1204_REG_WR(R25, \
    (((r25_reserved_0) << R25_RESERVED_0_OFF) & R25_RESERVED_0_MSK) |  \
    (((clk_div_rst) << CLK_DIV_RST_OFF) & CLK_DIV_RST_MSK) |  \
    (((clk_divclk_mult) << CLK_DIVCLK_MULT_OFF) & CLK_DIVCLK_MULT_MSK) |  \
    (((clk_mux) << CLK_MUX_OFF) & CLK_MUX_MSK))
// Register R28 [0x1c] -- R28

enum r28_fields_t {
    R28_RESERVED_0_OFF = 0xd,
    R28_RESERVED_0_MSK = 0xe000,
    FORCE_VCO_OFF = 0xc,
    FORCE_VCO_MSK = 0x1000,
    VCO_SEL_OFF = 0x9,
    VCO_SEL_MSK = 0xe00,
    R28_RESERVED_1_OFF = 0x0,
    R28_RESERVED_1_MSK = 0x1ff,
};
#define MAKE_LMX1204_R28(r28_reserved_0, force_vco, vco_sel, r28_reserved_1) MAKE_LMX1204_REG_WR(R28, \
    (((r28_reserved_0) << R28_RESERVED_0_OFF) & R28_RESERVED_0_MSK) |  \
    (((force_vco) << FORCE_VCO_OFF) & FORCE_VCO_MSK) |  \
    (((vco_sel) << VCO_SEL_OFF) & VCO_SEL_MSK) |  \
    (((r28_reserved_1) << R28_RESERVED_1_OFF) & R28_RESERVED_1_MSK))
// Register R29 [0x1d] -- R29

enum r29_fields_t {
    R29_RESERVED_0_OFF = 0xd,
    R29_RESERVED_0_MSK = 0xe000,
    R29_RESERVED_1_OFF = 0x8,
    R29_RESERVED_1_MSK = 0x1f00,
    CAPCTRL_OFF = 0x0,
    CAPCTRL_MSK = 0xff,
};
#define MAKE_LMX1204_R29(r29_reserved_0, r29_reserved_1, capctrl) MAKE_LMX1204_REG_WR(R29, \
    (((r29_reserved_0) << R29_RESERVED_0_OFF) & R29_RESERVED_0_MSK) |  \
    (((r29_reserved_1) << R29_RESERVED_1_OFF) & R29_RESERVED_1_MSK) |  \
    (((capctrl) << CAPCTRL_OFF) & CAPCTRL_MSK))
// Register R33 [0x21] -- R33

enum r33_fields_t {
    R33_RESERVED_0_OFF = 0x0,
    R33_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX1204_R33(r33_reserved_0) MAKE_LMX1204_REG_WR(R33, \
    (((r33_reserved_0) << R33_RESERVED_0_OFF) & R33_RESERVED_0_MSK))
// Register R34 [0x22] -- R34

enum r34_fields_t {
    R34_RESERVED_0_OFF = 0xe,
    R34_RESERVED_0_MSK = 0xc000,
    R34_RESERVED_1_OFF = 0x0,
    R34_RESERVED_1_MSK = 0x3fff,
};
#define MAKE_LMX1204_R34(r34_reserved_0, r34_reserved_1) MAKE_LMX1204_REG_WR(R34, \
    (((r34_reserved_0) << R34_RESERVED_0_OFF) & R34_RESERVED_0_MSK) |  \
    (((r34_reserved_1) << R34_RESERVED_1_OFF) & R34_RESERVED_1_MSK))
// Register R65 [0x41] -- R65
enum rb_vco_sel_options {
    RB_VCO_SEL_VCO5 = 15,
    RB_VCO_SEL_VCO4 = 23,
    RB_VCO_SEL_VCO3 = 27,
    RB_VCO_SEL_VCO2 = 29,
    RB_VCO_SEL_VCO1 = 30,
};

enum r65_fields_t {
    R65_RESERVED_0_OFF = 0x9,
    R65_RESERVED_0_MSK = 0xfe00,
    RB_VCO_SEL_OFF = 0x4,
    RB_VCO_SEL_MSK = 0x1f0,
    R65_RESERVED_1_OFF = 0x0,
    R65_RESERVED_1_MSK = 0xf,
};
#define MAKE_LMX1204_R65(r65_reserved_0, rb_vco_sel, r65_reserved_1) MAKE_LMX1204_REG_WR(R65, \
    (((r65_reserved_0) << R65_RESERVED_0_OFF) & R65_RESERVED_0_MSK) |  \
    (((rb_vco_sel) << RB_VCO_SEL_OFF) & RB_VCO_SEL_MSK) |  \
    (((r65_reserved_1) << R65_RESERVED_1_OFF) & R65_RESERVED_1_MSK))
// Register R67 [0x43] -- R67

enum r67_fields_t {
    R67_RESERVED_0_OFF = 0x0,
    R67_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX1204_R67(r67_reserved_0) MAKE_LMX1204_REG_WR(R67, \
    (((r67_reserved_0) << R67_RESERVED_0_OFF) & R67_RESERVED_0_MSK))
// Register R72 [0x48] -- R72
enum sysref_delay_bypass_options {
    SYSREF_DELAY_BYPASS_ENGAGE_IN_GENERATOR_MODE__BYPASS_IN_REPEATER_MODE = 0,
    SYSREF_DELAY_BYPASS_BYPASS_IN_ALL_MODES = 1,
    SYSREF_DELAY_BYPASS_ENGAGE_IN_ALL_MODES = 2,
};

enum r72_fields_t {
    R72_RESERVED_0_OFF = 0xf,
    R72_RESERVED_0_MSK = 0x8000,
    R72_RESERVED_1_OFF = 0x4,
    R72_RESERVED_1_MSK = 0x7ff0,
    PULSER_LATCH_OFF = 0x3,
    PULSER_LATCH_MSK = 0x8,
    SYSREFREQ_SPI_OFF = 0x2,
    SYSREFREQ_SPI_MSK = 0x4,
    SYSREF_DELAY_BYPASS_OFF = 0x0,
    SYSREF_DELAY_BYPASS_MSK = 0x3,
};
#define MAKE_LMX1204_R72(r72_reserved_0, r72_reserved_1, pulser_latch, sysrefreq_spi, sysref_delay_bypass) MAKE_LMX1204_REG_WR(R72, \
    (((r72_reserved_0) << R72_RESERVED_0_OFF) & R72_RESERVED_0_MSK) |  \
    (((r72_reserved_1) << R72_RESERVED_1_OFF) & R72_RESERVED_1_MSK) |  \
    (((pulser_latch) << PULSER_LATCH_OFF) & PULSER_LATCH_MSK) |  \
    (((sysrefreq_spi) << SYSREFREQ_SPI_OFF) & SYSREFREQ_SPI_MSK) |  \
    (((sysref_delay_bypass) << SYSREF_DELAY_BYPASS_OFF) & SYSREF_DELAY_BYPASS_MSK))
// Register R75 [0x4b] -- R75
enum rb_ld_options {
    RB_LD_UNLOCKED_VTUNE_LOW = 0,
    RB_LD_LOCKED = 2,
    RB_LD_UNLOCKED_VTUNE_HIGH = 3,
};

enum r75_fields_t {
    R75_RESERVED_0_OFF = 0xa,
    R75_RESERVED_0_MSK = 0xfc00,
    RB_LD_OFF = 0x8,
    RB_LD_MSK = 0x300,
    R75_RESERVED_1_OFF = 0x4,
    R75_RESERVED_1_MSK = 0xf0,
    R75_RESERVED_2_OFF = 0x0,
    R75_RESERVED_2_MSK = 0xf,
};
#define MAKE_LMX1204_R75(r75_reserved_0, rb_ld, r75_reserved_1, r75_reserved_2) MAKE_LMX1204_REG_WR(R75, \
    (((r75_reserved_0) << R75_RESERVED_0_OFF) & R75_RESERVED_0_MSK) |  \
    (((rb_ld) << RB_LD_OFF) & RB_LD_MSK) |  \
    (((r75_reserved_1) << R75_RESERVED_1_OFF) & R75_RESERVED_1_MSK) |  \
    (((r75_reserved_2) << R75_RESERVED_2_OFF) & R75_RESERVED_2_MSK))
// Register R79 [0x4f] -- R79

enum r79_fields_t {
    R79_RESERVED_0_OFF = 0xf,
    R79_RESERVED_0_MSK = 0x8000,
    R79_RESERVED_1_OFF = 0x0,
    R79_RESERVED_1_MSK = 0x7fff,
};
#define MAKE_LMX1204_R79(r79_reserved_0, r79_reserved_1) MAKE_LMX1204_REG_WR(R79, \
    (((r79_reserved_0) << R79_RESERVED_0_OFF) & R79_RESERVED_0_MSK) |  \
    (((r79_reserved_1) << R79_RESERVED_1_OFF) & R79_RESERVED_1_MSK))
// Register R86 [0x56] -- R86

enum r86_fields_t {
    R86_RESERVED_0_OFF = 0x0,
    R86_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX1204_R86(r86_reserved_0) MAKE_LMX1204_REG_WR(R86, \
    (((r86_reserved_0) << R86_RESERVED_0_OFF) & R86_RESERVED_0_MSK))
// Register R90 [0x5a] -- R90

enum r90_fields_t {
    R90_RESERVED_0_OFF = 0x8,
    R90_RESERVED_0_MSK = 0xff00,
    R90_RESERVED_1_OFF = 0x0,
    R90_RESERVED_1_MSK = 0xffff,
};
#define MAKE_LMX1204_R90(r90_reserved_0, r90_reserved_1) MAKE_LMX1204_REG_WR(R90, \
    (((r90_reserved_0) << R90_RESERVED_0_OFF) & R90_RESERVED_0_MSK) |  \
    (((r90_reserved_1) << R90_RESERVED_1_OFF) & R90_RESERVED_1_MSK))
