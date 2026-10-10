enum lmx1214_regs_t {
    R0 = 0x0,
    R2 = 0x2,
    R3 = 0x3,
    R4 = 0x4,
    R5 = 0x5,
    R7 = 0x7,
    R8 = 0x8,
    R9 = 0x9,
    R11 = 0xb,
    R12 = 0xc,
    R13 = 0xd,
    R14 = 0xe,
    R15 = 0xf,
    R23 = 0x17,
    R24 = 0x18,
    R25 = 0x19,
    R75 = 0x4b,
    R79 = 0x4f,
    R86 = 0x56,
    R90 = 0x5a,
};
#define MAKE_LMX1214_REG_WR(a, v) (((a) << 16) | ((v) & 0xffff))
#define MAKE_LMX1214_REG_RD(a) (0x800000 | ((a) << 16))
// Register R0 [0x0] -- R0

enum r0_fields_t {
    UNDISCLOSED_OFF = 0x3,
    UNDISCLOSED_MSK = 0xfff8,
    POWERDOWN_OFF = 0x2,
    POWERDOWN_MSK = 0x4,
    R0_UNDISCLOSED_OFF = 0x0,
    R0_UNDISCLOSED_MSK = 0x3,
};
#define MAKE_LMX1214_R0(undisclosed, powerdown, r0_undisclosed) MAKE_LMX1214_REG_WR(R0, \
    (((undisclosed) << UNDISCLOSED_OFF) & UNDISCLOSED_MSK) |  \
    (((powerdown) << POWERDOWN_OFF) & POWERDOWN_MSK) |  \
    (((r0_undisclosed) << R0_UNDISCLOSED_OFF) & R0_UNDISCLOSED_MSK))
// Register R2 [0x2] -- R2

enum r2_fields_t {
    R2_UNDISCLOSED_OFF = 0xb,
    R2_UNDISCLOSED_MSK = 0xf800,
    R2_UNDISCLOSED_0_OFF = 0x6,
    R2_UNDISCLOSED_0_MSK = 0x7c0,
    SMCLK_EN_OFF = 0x5,
    SMCLK_EN_MSK = 0x20,
    R2_UNDISCLOSED_1_OFF = 0x0,
    R2_UNDISCLOSED_1_MSK = 0x1f,
};
#define MAKE_LMX1214_R2(r2_undisclosed, r2_undisclosed_0, smclk_en, r2_undisclosed_1) MAKE_LMX1214_REG_WR(R2, \
    (((r2_undisclosed) << R2_UNDISCLOSED_OFF) & R2_UNDISCLOSED_MSK) |  \
    (((r2_undisclosed_0) << R2_UNDISCLOSED_0_OFF) & R2_UNDISCLOSED_0_MSK) |  \
    (((smclk_en) << SMCLK_EN_OFF) & SMCLK_EN_MSK) |  \
    (((r2_undisclosed_1) << R2_UNDISCLOSED_1_OFF) & R2_UNDISCLOSED_1_MSK))
// Register R3 [0x3] -- R3

enum r3_fields_t {
    CLKOUT3_EN_OFF = 0xf,
    CLKOUT3_EN_MSK = 0x8000,
    CLKOUT2_EN_OFF = 0xe,
    CLKOUT2_EN_MSK = 0x4000,
    CLKOUT1_EN_OFF = 0xd,
    CLKOUT1_EN_MSK = 0x2000,
    CLKOUT0_EN_OFF = 0xc,
    CLKOUT0_EN_MSK = 0x1000,
    R3_UNDISCLOSED_OFF = 0x0,
    R3_UNDISCLOSED_MSK = 0xfff,
};
#define MAKE_LMX1214_R3(clkout3_en, clkout2_en, clkout1_en, clkout0_en, r3_undisclosed) MAKE_LMX1214_REG_WR(R3, \
    (((clkout3_en) << CLKOUT3_EN_OFF) & CLKOUT3_EN_MSK) |  \
    (((clkout2_en) << CLKOUT2_EN_OFF) & CLKOUT2_EN_MSK) |  \
    (((clkout1_en) << CLKOUT1_EN_OFF) & CLKOUT1_EN_MSK) |  \
    (((clkout0_en) << CLKOUT0_EN_OFF) & CLKOUT0_EN_MSK) |  \
    (((r3_undisclosed) << R3_UNDISCLOSED_OFF) & R3_UNDISCLOSED_MSK))
// Register R4 [0x4] -- R4

enum r4_fields_t {
    R4_UNDISCLOSED_OFF = 0xe,
    R4_UNDISCLOSED_MSK = 0xc000,
    CLKOUT1_PWR_OFF = 0xb,
    CLKOUT1_PWR_MSK = 0x3800,
    CLKOUT0_PWR_OFF = 0x8,
    CLKOUT0_PWR_MSK = 0x700,
    R4_UNDISCLOSED_0_OFF = 0x0,
    R4_UNDISCLOSED_0_MSK = 0xff,
};
#define MAKE_LMX1214_R4(r4_undisclosed, clkout1_pwr, clkout0_pwr, r4_undisclosed_0) MAKE_LMX1214_REG_WR(R4, \
    (((r4_undisclosed) << R4_UNDISCLOSED_OFF) & R4_UNDISCLOSED_MSK) |  \
    (((clkout1_pwr) << CLKOUT1_PWR_OFF) & CLKOUT1_PWR_MSK) |  \
    (((clkout0_pwr) << CLKOUT0_PWR_OFF) & CLKOUT0_PWR_MSK) |  \
    (((r4_undisclosed_0) << R4_UNDISCLOSED_0_OFF) & R4_UNDISCLOSED_0_MSK))
// Register R5 [0x5] -- R5

enum r5_fields_t {
    R5_UNDISCLOSED_OFF = 0xf,
    R5_UNDISCLOSED_MSK = 0x8000,
    R5_UNDISCLOSED_0_OFF = 0x6,
    R5_UNDISCLOSED_0_MSK = 0x7fc0,
    CLKOUT3_PWR_OFF = 0x3,
    CLKOUT3_PWR_MSK = 0x38,
    CLKOUT2_PWR_OFF = 0x0,
    CLKOUT2_PWR_MSK = 0x7,
};
#define MAKE_LMX1214_R5(r5_undisclosed, r5_undisclosed_0, clkout3_pwr, clkout2_pwr) MAKE_LMX1214_REG_WR(R5, \
    (((r5_undisclosed) << R5_UNDISCLOSED_OFF) & R5_UNDISCLOSED_MSK) |  \
    (((r5_undisclosed_0) << R5_UNDISCLOSED_0_OFF) & R5_UNDISCLOSED_0_MSK) |  \
    (((clkout3_pwr) << CLKOUT3_PWR_OFF) & CLKOUT3_PWR_MSK) |  \
    (((clkout2_pwr) << CLKOUT2_PWR_OFF) & CLKOUT2_PWR_MSK))
// Register R7 [0x7] -- R7
enum auxclkout_vcm_options {
    AUXCLKOUT_VCM_1_2V = 0,
    AUXCLKOUT_VCM_1_1V = 1,
    AUXCLKOUT_VCM_1_0V = 2,
    AUXCLKOUT_VCM_0_9V = 3,
};

enum r7_fields_t {
    R7_UNDISCLOSED_OFF = 0xf,
    R7_UNDISCLOSED_MSK = 0x8000,
    R7_UNDISCLOSED_0_OFF = 0xd,
    R7_UNDISCLOSED_0_MSK = 0x6000,
    AUXCLKOUT_VCM_OFF = 0xb,
    AUXCLKOUT_VCM_MSK = 0x1800,
    R7_UNDISCLOSED_1_OFF = 0x9,
    R7_UNDISCLOSED_1_MSK = 0x600,
    AUXCLK_DIV_PWR_PRE_OFF = 0x7,
    AUXCLK_DIV_PWR_PRE_MSK = 0x180,
    R7_UNDISCLOSED_2_OFF = 0x4,
    R7_UNDISCLOSED_2_MSK = 0x70,
    AUXCLKOUT_PWR_OFF = 0x1,
    AUXCLKOUT_PWR_MSK = 0xe,
    R7_UNDISCLOSED_3_OFF = 0x0,
    R7_UNDISCLOSED_3_MSK = 0x1,
};
#define MAKE_LMX1214_R7(r7_undisclosed, r7_undisclosed_0, auxclkout_vcm, r7_undisclosed_1, auxclk_div_pwr_pre, r7_undisclosed_2, auxclkout_pwr, r7_undisclosed_3) MAKE_LMX1214_REG_WR(R7, \
    (((r7_undisclosed) << R7_UNDISCLOSED_OFF) & R7_UNDISCLOSED_MSK) |  \
    (((r7_undisclosed_0) << R7_UNDISCLOSED_0_OFF) & R7_UNDISCLOSED_0_MSK) |  \
    (((auxclkout_vcm) << AUXCLKOUT_VCM_OFF) & AUXCLKOUT_VCM_MSK) |  \
    (((r7_undisclosed_1) << R7_UNDISCLOSED_1_OFF) & R7_UNDISCLOSED_1_MSK) |  \
    (((auxclk_div_pwr_pre) << AUXCLK_DIV_PWR_PRE_OFF) & AUXCLK_DIV_PWR_PRE_MSK) |  \
    (((r7_undisclosed_2) << R7_UNDISCLOSED_2_OFF) & R7_UNDISCLOSED_2_MSK) |  \
    (((auxclkout_pwr) << AUXCLKOUT_PWR_OFF) & AUXCLKOUT_PWR_MSK) |  \
    (((r7_undisclosed_3) << R7_UNDISCLOSED_3_OFF) & R7_UNDISCLOSED_3_MSK))
// Register R8 [0x8] -- R8
enum auxclk_div_pre_options {
    AUXCLK_DIV_PRE_DIV1 = 1,
    AUXCLK_DIV_PRE_DIV2 = 2,
    AUXCLK_DIV_PRE_DIV4 = 4,
};
enum auxclkout_fmt_options {
    AUXCLKOUT_FMT_LVDS = 0,
    AUXCLKOUT_FMT_CML = 2,
};

enum r8_fields_t {
    R8_UNDISCLOSED_OFF = 0x9,
    R8_UNDISCLOSED_MSK = 0xfe00,
    AUXCLK_DIV_PRE_OFF = 0x6,
    AUXCLK_DIV_PRE_MSK = 0x1c0,
    R8_UNDISCLOSED_5_OFF = 0x5,
    R8_UNDISCLOSED_5_MSK = 0x20,
    AUXCLKOUT_EN_OFF = 0x4,
    AUXCLKOUT_EN_MSK = 0x10,
    R8_UNDISCLOSED_0_OFF = 0x2,
    R8_UNDISCLOSED_0_MSK = 0xc,
    AUXCLKOUT_FMT_OFF = 0x0,
    AUXCLKOUT_FMT_MSK = 0x3,
};
#define MAKE_LMX1214_R8(r8_undisclosed, auxclk_div_pre, r8_undisclosed_5, auxclkout_en, r8_undisclosed_0, auxclkout_fmt) MAKE_LMX1214_REG_WR(R8, \
    (((r8_undisclosed) << R8_UNDISCLOSED_OFF) & R8_UNDISCLOSED_MSK) |  \
    (((auxclk_div_pre) << AUXCLK_DIV_PRE_OFF) & AUXCLK_DIV_PRE_MSK) |  \
    (((r8_undisclosed_5) << R8_UNDISCLOSED_5_OFF) & R8_UNDISCLOSED_5_MSK) |  \
    (((auxclkout_en) << AUXCLKOUT_EN_OFF) & AUXCLKOUT_EN_MSK) |  \
    (((r8_undisclosed_0) << R8_UNDISCLOSED_0_OFF) & R8_UNDISCLOSED_0_MSK) |  \
    (((auxclkout_fmt) << AUXCLKOUT_FMT_OFF) & AUXCLKOUT_FMT_MSK))
// Register R9 [0x9] -- R9
enum sync_vcm_options {
    SYNC_VCM_1_3V = 0,
    SYNC_VCM_1_1V = 1,
    SYNC_VCM_1_5V = 2,
    SYNC_VCM_DISABLED = 3,
};
enum auxclk_div_options {
    AUXCLK_DIV_DIV2 = 2,
    AUXCLK_DIV_DIV3 = 3,
    AUXCLK_DIV_DIV1023 = 1023,
};

enum r9_fields_t {
    SYNC_VCM_OFF = 0xe,
    SYNC_VCM_MSK = 0xc000,
    SYNC_EN_OFF = 0xd,
    SYNC_EN_MSK = 0x2000,
    R9_UNDISCLOSED_OFF = 0xc,
    R9_UNDISCLOSED_MSK = 0x1000,
    AUXCLK_DIV_BYP_OFF = 0xb,
    AUXCLK_DIV_BYP_MSK = 0x800,
    R9_UNDISCLOSED_0_OFF = 0xa,
    R9_UNDISCLOSED_0_MSK = 0x400,
    AUXCLK_DIV_OFF = 0x0,
    AUXCLK_DIV_MSK = 0x3ff,
};
#define MAKE_LMX1214_R9(sync_vcm, sync_en, r9_undisclosed, auxclk_div_byp, r9_undisclosed_0, auxclk_div) MAKE_LMX1214_REG_WR(R9, \
    (((sync_vcm) << SYNC_VCM_OFF) & SYNC_VCM_MSK) |  \
    (((sync_en) << SYNC_EN_OFF) & SYNC_EN_MSK) |  \
    (((r9_undisclosed) << R9_UNDISCLOSED_OFF) & R9_UNDISCLOSED_MSK) |  \
    (((auxclk_div_byp) << AUXCLK_DIV_BYP_OFF) & AUXCLK_DIV_BYP_MSK) |  \
    (((r9_undisclosed_0) << R9_UNDISCLOSED_0_OFF) & R9_UNDISCLOSED_0_MSK) |  \
    (((auxclk_div) << AUXCLK_DIV_OFF) & AUXCLK_DIV_MSK))
// Register R11 [0xb] -- R11

enum r11_fields_t {
    RB_CLKPOS_L_OFF = 0x0,
    RB_CLKPOS_L_MSK = 0xffff,
};
#define MAKE_LMX1214_R11(rb_clkpos_l) MAKE_LMX1214_REG_WR(R11, \
    (((rb_clkpos_l) << RB_CLKPOS_L_OFF) & RB_CLKPOS_L_MSK))
// Register R12 [0xc] -- R12

enum r12_fields_t {
    RB_CLKPOS_U_OFF = 0x0,
    RB_CLKPOS_U_MSK = 0xffff,
};
#define MAKE_LMX1214_R12(rb_clkpos_u) MAKE_LMX1214_REG_WR(R12, \
    (((rb_clkpos_u) << RB_CLKPOS_U_OFF) & RB_CLKPOS_U_MSK))
// Register R13 [0xd] -- R13
enum sync_dly_step_options {
    SYNC_DLY_STEP_28_PS_1_4GHZ_TO_2_7GHZ = 0,
    SYNC_DLY_STEP_15_PS__2_4GHZ_TO_4_7GHZ = 1,
    SYNC_DLY_STEP_11_PS_3_1GHZ_TO_5_7GHZ = 2,
    SYNC_DLY_STEP_8_PS_4_5GHZ_TO_12_8GHZ = 3,
};

enum r13_fields_t {
    R13_UNDISCLOSED_OFF = 0x2,
    R13_UNDISCLOSED_MSK = 0xfffc,
    SYNC_DLY_STEP_OFF = 0x0,
    SYNC_DLY_STEP_MSK = 0x3,
};
#define MAKE_LMX1214_R13(r13_undisclosed, sync_dly_step) MAKE_LMX1214_REG_WR(R13, \
    (((r13_undisclosed) << R13_UNDISCLOSED_OFF) & R13_UNDISCLOSED_MSK) |  \
    (((sync_dly_step) << SYNC_DLY_STEP_OFF) & SYNC_DLY_STEP_MSK))
// Register R14 [0xe] -- R14

enum r14_fields_t {
    R14_UNDISCLOSED_OFF = 0x3,
    R14_UNDISCLOSED_MSK = 0xfff8,
    CLKPOS_CAPTURE_EN_OFF = 0x2,
    CLKPOS_CAPTURE_EN_MSK = 0x4,
    R14_UNDISCLOSED_0_OFF = 0x1,
    R14_UNDISCLOSED_0_MSK = 0x2,
    SYNC_LATCH_OFF = 0x0,
    SYNC_LATCH_MSK = 0x1,
};
#define MAKE_LMX1214_R14(r14_undisclosed, clkpos_capture_en, r14_undisclosed_0, sync_latch) MAKE_LMX1214_REG_WR(R14, \
    (((r14_undisclosed) << R14_UNDISCLOSED_OFF) & R14_UNDISCLOSED_MSK) |  \
    (((clkpos_capture_en) << CLKPOS_CAPTURE_EN_OFF) & CLKPOS_CAPTURE_EN_MSK) |  \
    (((r14_undisclosed_0) << R14_UNDISCLOSED_0_OFF) & R14_UNDISCLOSED_0_MSK) |  \
    (((sync_latch) << SYNC_LATCH_OFF) & SYNC_LATCH_MSK))
// Register R15 [0xf] -- R15

enum r15_fields_t {
    R15_UNDISCLOSED_OFF = 0xc,
    R15_UNDISCLOSED_MSK = 0xf000,
    R15_UNDISCLOSED_0_OFF = 0x7,
    R15_UNDISCLOSED_0_MSK = 0xf80,
    SYNC_DLY_OFF = 0x1,
    SYNC_DLY_MSK = 0x7e,
    SYNC_CLR_OFF = 0x0,
    SYNC_CLR_MSK = 0x1,
};
#define MAKE_LMX1214_R15(r15_undisclosed, r15_undisclosed_0, sync_dly, sync_clr) MAKE_LMX1214_REG_WR(R15, \
    (((r15_undisclosed) << R15_UNDISCLOSED_OFF) & R15_UNDISCLOSED_MSK) |  \
    (((r15_undisclosed_0) << R15_UNDISCLOSED_0_OFF) & R15_UNDISCLOSED_0_MSK) |  \
    (((sync_dly) << SYNC_DLY_OFF) & SYNC_DLY_MSK) |  \
    (((sync_clr) << SYNC_CLR_OFF) & SYNC_CLR_MSK))
// Register R23 [0x17] -- R23
enum muxout_en_options {
    MUXOUT_EN_TRI_STATES = 0,
    MUXOUT_EN_PUSH_PULL = 1,
};

enum r23_fields_t {
    TS_EN_OFF = 0xf,
    TS_EN_MSK = 0x8000,
    R23_UNDISCLOSED_OFF = 0xe,
    R23_UNDISCLOSED_MSK = 0x4000,
    MUXOUT_EN_OFF = 0xd,
    MUXOUT_EN_MSK = 0x2000,
    R23_UNDISCLOSED_0_OFF = 0x0,
    R23_UNDISCLOSED_0_MSK = 0x1fff,
};
#define MAKE_LMX1214_R23(ts_en, r23_undisclosed, muxout_en, r23_undisclosed_0) MAKE_LMX1214_REG_WR(R23, \
    (((ts_en) << TS_EN_OFF) & TS_EN_MSK) |  \
    (((r23_undisclosed) << R23_UNDISCLOSED_OFF) & R23_UNDISCLOSED_MSK) |  \
    (((muxout_en) << MUXOUT_EN_OFF) & MUXOUT_EN_MSK) |  \
    (((r23_undisclosed_0) << R23_UNDISCLOSED_0_OFF) & R23_UNDISCLOSED_0_MSK))
// Register R24 [0x18] -- R24

enum r24_fields_t {
    R24_UNDISCLOSED_OFF = 0xe,
    R24_UNDISCLOSED_MSK = 0xc000,
    R24_UNDISCLOSED_0_OFF = 0xc,
    R24_UNDISCLOSED_0_MSK = 0x3000,
    RB_TS_OFF = 0x1,
    RB_TS_MSK = 0xffe,
    TS_CNT_EN_OFF = 0x0,
    TS_CNT_EN_MSK = 0x1,
};
#define MAKE_LMX1214_R24(r24_undisclosed, r24_undisclosed_0, rb_ts, ts_cnt_en) MAKE_LMX1214_REG_WR(R24, \
    (((r24_undisclosed) << R24_UNDISCLOSED_OFF) & R24_UNDISCLOSED_MSK) |  \
    (((r24_undisclosed_0) << R24_UNDISCLOSED_0_OFF) & R24_UNDISCLOSED_0_MSK) |  \
    (((rb_ts) << RB_TS_OFF) & RB_TS_MSK) |  \
    (((ts_cnt_en) << TS_CNT_EN_OFF) & TS_CNT_EN_MSK))
// Register R25 [0x19] -- R25
enum clk_mux_options {
    CLK_MUX_BUFFER = 1,
    CLK_MUX_DIVIDER = 2,
};

enum r25_fields_t {
    R25_UNDISCLOSED_OFF = 0x7,
    R25_UNDISCLOSED_MSK = 0xff80,
    CLK_DIV_RST_OFF = 0x6,
    CLK_DIV_RST_MSK = 0x40,
    CLK_DIV_OFF = 0x3,
    CLK_DIV_MSK = 0x38,
    CLK_MUX_OFF = 0x0,
    CLK_MUX_MSK = 0x7,
};
#define MAKE_LMX1214_R25(r25_undisclosed, clk_div_rst, clk_div, clk_mux) MAKE_LMX1214_REG_WR(R25, \
    (((r25_undisclosed) << R25_UNDISCLOSED_OFF) & R25_UNDISCLOSED_MSK) |  \
    (((clk_div_rst) << CLK_DIV_RST_OFF) & CLK_DIV_RST_MSK) |  \
    (((clk_div) << CLK_DIV_OFF) & CLK_DIV_MSK) |  \
    (((clk_mux) << CLK_MUX_OFF) & CLK_MUX_MSK))
// Register R75 [0x4b] -- R75

enum r75_fields_t {
    RB_CLKOUT2_EN_OFF = 0xf,
    RB_CLKOUT2_EN_MSK = 0x8000,
    RB_CLKOUT1_EN_OFF = 0xe,
    RB_CLKOUT1_EN_MSK = 0x4000,
    RB_CLKOUT0_EN_OFF = 0xd,
    RB_CLKOUT0_EN_MSK = 0x2000,
    RB_MUXSEL1_OFF = 0xc,
    RB_MUXSEL1_MSK = 0x1000,
    R75_UNDISCLOSED_OFF = 0x7,
    R75_UNDISCLOSED_MSK = 0xf80,
    RB_DIVSEL1_OFF = 0x6,
    RB_DIVSEL1_MSK = 0x40,
    RB_DIVSEL0_OFF = 0x5,
    RB_DIVSEL0_MSK = 0x20,
    RB_CE_OFF = 0x4,
    RB_CE_MSK = 0x10,
    R75_UNDISCLOSED_0_OFF = 0x0,
    R75_UNDISCLOSED_0_MSK = 0xf,
};
#define MAKE_LMX1214_R75(rb_clkout2_en, rb_clkout1_en, rb_clkout0_en, rb_muxsel1, r75_undisclosed, rb_divsel1, rb_divsel0, rb_ce, r75_undisclosed_0) MAKE_LMX1214_REG_WR(R75, \
    (((rb_clkout2_en) << RB_CLKOUT2_EN_OFF) & RB_CLKOUT2_EN_MSK) |  \
    (((rb_clkout1_en) << RB_CLKOUT1_EN_OFF) & RB_CLKOUT1_EN_MSK) |  \
    (((rb_clkout0_en) << RB_CLKOUT0_EN_OFF) & RB_CLKOUT0_EN_MSK) |  \
    (((rb_muxsel1) << RB_MUXSEL1_OFF) & RB_MUXSEL1_MSK) |  \
    (((r75_undisclosed) << R75_UNDISCLOSED_OFF) & R75_UNDISCLOSED_MSK) |  \
    (((rb_divsel1) << RB_DIVSEL1_OFF) & RB_DIVSEL1_MSK) |  \
    (((rb_divsel0) << RB_DIVSEL0_OFF) & RB_DIVSEL0_MSK) |  \
    (((rb_ce) << RB_CE_OFF) & RB_CE_MSK) |  \
    (((r75_undisclosed_0) << R75_UNDISCLOSED_0_OFF) & R75_UNDISCLOSED_0_MSK))
// Register R79 [0x4f] -- R79

enum r79_fields_t {
    R79_UNDISCLOSED_OFF = 0xf,
    R79_UNDISCLOSED_MSK = 0x8000,
    R79_UNDISCLOSED_0_OFF = 0x0,
    R79_UNDISCLOSED_0_MSK = 0x7fff,
};
#define MAKE_LMX1214_R79(r79_undisclosed, r79_undisclosed_0) MAKE_LMX1214_REG_WR(R79, \
    (((r79_undisclosed) << R79_UNDISCLOSED_OFF) & R79_UNDISCLOSED_MSK) |  \
    (((r79_undisclosed_0) << R79_UNDISCLOSED_0_OFF) & R79_UNDISCLOSED_0_MSK))
// Register R86 [0x56] -- R86

enum r86_fields_t {
    R86_UNDISCLOSED_OFF = 0x3,
    R86_UNDISCLOSED_MSK = 0xfff8,
    MUXOUT_EN_OVRD_OFF = 0x2,
    MUXOUT_EN_OVRD_MSK = 0x4,
    R86_UNDISCLOSED_0_OFF = 0x0,
    R86_UNDISCLOSED_0_MSK = 0x3,
};
#define MAKE_LMX1214_R86(r86_undisclosed, muxout_en_ovrd, r86_undisclosed_0) MAKE_LMX1214_REG_WR(R86, \
    (((r86_undisclosed) << R86_UNDISCLOSED_OFF) & R86_UNDISCLOSED_MSK) |  \
    (((muxout_en_ovrd) << MUXOUT_EN_OVRD_OFF) & MUXOUT_EN_OVRD_MSK) |  \
    (((r86_undisclosed_0) << R86_UNDISCLOSED_0_OFF) & R86_UNDISCLOSED_0_MSK))
// Register R90 [0x5a] -- R90

enum r90_fields_t {
    R90_UNDISCLOSED_OFF = 0x8,
    R90_UNDISCLOSED_MSK = 0xff00,
    R90_UNDISCLOSED_0_OFF = 0x7,
    R90_UNDISCLOSED_0_MSK = 0x80,
    AUXCLK_DIV_BYP3_OFF = 0x6,
    AUXCLK_DIV_BYP3_MSK = 0x40,
    AUXCLK_DIV_BYP2_OFF = 0x5,
    AUXCLK_DIV_BYP2_MSK = 0x20,
    R90_UNDISCLOSED_1_OFF = 0x0,
    R90_UNDISCLOSED_1_MSK = 0x1f,
};
#define MAKE_LMX1214_R90(r90_undisclosed, r90_undisclosed_0, auxclk_div_byp3, auxclk_div_byp2, r90_undisclosed_1) MAKE_LMX1214_REG_WR(R90, \
    (((r90_undisclosed) << R90_UNDISCLOSED_OFF) & R90_UNDISCLOSED_MSK) |  \
    (((r90_undisclosed_0) << R90_UNDISCLOSED_0_OFF) & R90_UNDISCLOSED_0_MSK) |  \
    (((auxclk_div_byp3) << AUXCLK_DIV_BYP3_OFF) & AUXCLK_DIV_BYP3_MSK) |  \
    (((auxclk_div_byp2) << AUXCLK_DIV_BYP2_OFF) & AUXCLK_DIV_BYP2_MSK) |  \
    (((r90_undisclosed_1) << R90_UNDISCLOSED_1_OFF) & R90_UNDISCLOSED_1_MSK))
