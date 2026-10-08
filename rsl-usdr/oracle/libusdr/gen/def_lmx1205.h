enum lmx1205_regs_t {
    R0 = 0x0,
    R1 = 0x1,
    R2 = 0x2,
    R3 = 0x3,
    R4 = 0x4,
    R5 = 0x5,
    R6 = 0x6,
    R7 = 0x7,
    R8 = 0x8,
    R9 = 0x9,
    R10 = 0xa,
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
    R26 = 0x1a,
    R27 = 0x1b,
    R29 = 0x1d,
    R30 = 0x1e,
    R31 = 0x1f,
    R32 = 0x20,
    R36 = 0x24,
    R37 = 0x25,
    R39 = 0x27,
    R40 = 0x28,
    R41 = 0x29,
    R42 = 0x2a,
    R43 = 0x2b,
    R44 = 0x2c,
    R45 = 0x2d,
    R54 = 0x36,
    R55 = 0x37,
    R77 = 0x4d,
};
#define MAKE_LMX1205_REG_WR(a, v) (0x80000000 | ((a) << 16) | ((v) & 0xffff))
#define MAKE_LMX1205_REG_RD(a) (((a) << 16))
// Register R0 [0x0] -- R0

enum r0_fields_t {
    UNDISCLOSED_OFF = 0x2,
    UNDISCLOSED_MSK = 0xfffc,
    POWERDOWN_OFF = 0x1,
    POWERDOWN_MSK = 0x2,
    RESET_OFF = 0x0,
    RESET_MSK = 0x1,
};
#define MAKE_LMX1205_R0(undisclosed, powerdown, reset) MAKE_LMX1205_REG_WR(R0, \
    (((undisclosed) << UNDISCLOSED_OFF) & UNDISCLOSED_MSK) |  \
    (((powerdown) << POWERDOWN_OFF) & POWERDOWN_MSK) |  \
    (((reset) << RESET_OFF) & RESET_MSK))
// Register R1 [0x1] -- R1
enum ld_dis_options {
    LD_DIS_LOCK_DETECT = 0,
    LD_DIS_READBACK = 1,
};

enum r1_fields_t {
    R1_UNDISCLOSED_OFF = 0x5,
    R1_UNDISCLOSED_MSK = 0xffe0,
    LD_DIS_OFF = 0x4,
    LD_DIS_MSK = 0x10,
    READBACK_CTRL_OFF = 0x3,
    READBACK_CTRL_MSK = 0x8,
    R1_UNDISCLOSED_0_OFF = 0x0,
    R1_UNDISCLOSED_0_MSK = 0x7,
};
#define MAKE_LMX1205_R1(r1_undisclosed, ld_dis, readback_ctrl, r1_undisclosed_0) MAKE_LMX1205_REG_WR(R1, \
    (((r1_undisclosed) << R1_UNDISCLOSED_OFF) & R1_UNDISCLOSED_MSK) |  \
    (((ld_dis) << LD_DIS_OFF) & LD_DIS_MSK) |  \
    (((readback_ctrl) << READBACK_CTRL_OFF) & READBACK_CTRL_MSK) |  \
    (((r1_undisclosed_0) << R1_UNDISCLOSED_0_OFF) & R1_UNDISCLOSED_0_MSK))
// Register R2 [0x2] -- R2

enum r2_fields_t {
    R2_UNDISCLOSED_OFF = 0xa,
    R2_UNDISCLOSED_MSK = 0xfc00,
    TEMPSENSE_EN_OFF = 0x9,
    TEMPSENSE_EN_MSK = 0x200,
    SYNC_EN_OFF = 0x8,
    SYNC_EN_MSK = 0x100,
    R2_UNDISCLOSED_0_OFF = 0x7,
    R2_UNDISCLOSED_0_MSK = 0x80,
    SYSREF_EN_OFF = 0x6,
    SYSREF_EN_MSK = 0x40,
    R2_UNDISCLOSED_1_OFF = 0x5,
    R2_UNDISCLOSED_1_MSK = 0x20,
    LOGIC_EN_OFF = 0x4,
    LOGIC_EN_MSK = 0x10,
    CH3_EN_OFF = 0x3,
    CH3_EN_MSK = 0x8,
    CH2_EN_OFF = 0x2,
    CH2_EN_MSK = 0x4,
    CH1_EN_OFF = 0x1,
    CH1_EN_MSK = 0x2,
    CH0_EN_OFF = 0x0,
    CH0_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R2(r2_undisclosed, tempsense_en, sync_en, r2_undisclosed_0, sysref_en, r2_undisclosed_1, logic_en, ch3_en, ch2_en, ch1_en, ch0_en) MAKE_LMX1205_REG_WR(R2, \
    (((r2_undisclosed) << R2_UNDISCLOSED_OFF) & R2_UNDISCLOSED_MSK) |  \
    (((tempsense_en) << TEMPSENSE_EN_OFF) & TEMPSENSE_EN_MSK) |  \
    (((sync_en) << SYNC_EN_OFF) & SYNC_EN_MSK) |  \
    (((r2_undisclosed_0) << R2_UNDISCLOSED_0_OFF) & R2_UNDISCLOSED_0_MSK) |  \
    (((sysref_en) << SYSREF_EN_OFF) & SYSREF_EN_MSK) |  \
    (((r2_undisclosed_1) << R2_UNDISCLOSED_1_OFF) & R2_UNDISCLOSED_1_MSK) |  \
    (((logic_en) << LOGIC_EN_OFF) & LOGIC_EN_MSK) |  \
    (((ch3_en) << CH3_EN_OFF) & CH3_EN_MSK) |  \
    (((ch2_en) << CH2_EN_OFF) & CH2_EN_MSK) |  \
    (((ch1_en) << CH1_EN_OFF) & CH1_EN_MSK) |  \
    (((ch0_en) << CH0_EN_OFF) & CH0_EN_MSK))
// Register R3 [0x3] -- R3

enum r3_fields_t {
    R3_UNDISCLOSED_OFF = 0x7,
    R3_UNDISCLOSED_MSK = 0xff80,
    CLKIN_DLY_OFF = 0x0,
    CLKIN_DLY_MSK = 0x7f,
};
#define MAKE_LMX1205_R3(r3_undisclosed, clkin_dly) MAKE_LMX1205_REG_WR(R3, \
    (((r3_undisclosed) << R3_UNDISCLOSED_OFF) & R3_UNDISCLOSED_MSK) |  \
    (((clkin_dly) << CLKIN_DLY_OFF) & CLKIN_DLY_MSK))
// Register R4 [0x4] -- R4

enum r4_fields_t {
    R4_UNDISCLOSED_OFF = 0xb,
    R4_UNDISCLOSED_MSK = 0xf800,
    CLK0_DLY_OFF = 0x4,
    CLK0_DLY_MSK = 0x7f0,
    CLK0_PWR_OFF = 0x1,
    CLK0_PWR_MSK = 0xe,
    CLK0_EN_OFF = 0x0,
    CLK0_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R4(r4_undisclosed, clk0_dly, clk0_pwr, clk0_en) MAKE_LMX1205_REG_WR(R4, \
    (((r4_undisclosed) << R4_UNDISCLOSED_OFF) & R4_UNDISCLOSED_MSK) |  \
    (((clk0_dly) << CLK0_DLY_OFF) & CLK0_DLY_MSK) |  \
    (((clk0_pwr) << CLK0_PWR_OFF) & CLK0_PWR_MSK) |  \
    (((clk0_en) << CLK0_EN_OFF) & CLK0_EN_MSK))
// Register R5 [0x5] -- R5

enum r5_fields_t {
    R5_UNDISCLOSED_OFF = 0xb,
    R5_UNDISCLOSED_MSK = 0xf800,
    CLK1_DLY_OFF = 0x4,
    CLK1_DLY_MSK = 0x7f0,
    CLK1_PWR_OFF = 0x1,
    CLK1_PWR_MSK = 0xe,
    CLK1_EN_OFF = 0x0,
    CLK1_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R5(r5_undisclosed, clk1_dly, clk1_pwr, clk1_en) MAKE_LMX1205_REG_WR(R5, \
    (((r5_undisclosed) << R5_UNDISCLOSED_OFF) & R5_UNDISCLOSED_MSK) |  \
    (((clk1_dly) << CLK1_DLY_OFF) & CLK1_DLY_MSK) |  \
    (((clk1_pwr) << CLK1_PWR_OFF) & CLK1_PWR_MSK) |  \
    (((clk1_en) << CLK1_EN_OFF) & CLK1_EN_MSK))
// Register R6 [0x6] -- R6

enum r6_fields_t {
    R6_UNDISCLOSED_OFF = 0xb,
    R6_UNDISCLOSED_MSK = 0xf800,
    CLK2_DLY_OFF = 0x4,
    CLK2_DLY_MSK = 0x7f0,
    CLK2_PWR_OFF = 0x1,
    CLK2_PWR_MSK = 0xe,
    CLK2_EN_OFF = 0x0,
    CLK2_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R6(r6_undisclosed, clk2_dly, clk2_pwr, clk2_en) MAKE_LMX1205_REG_WR(R6, \
    (((r6_undisclosed) << R6_UNDISCLOSED_OFF) & R6_UNDISCLOSED_MSK) |  \
    (((clk2_dly) << CLK2_DLY_OFF) & CLK2_DLY_MSK) |  \
    (((clk2_pwr) << CLK2_PWR_OFF) & CLK2_PWR_MSK) |  \
    (((clk2_en) << CLK2_EN_OFF) & CLK2_EN_MSK))
// Register R7 [0x7] -- R7

enum r7_fields_t {
    R7_UNDISCLOSED_OFF = 0xb,
    R7_UNDISCLOSED_MSK = 0xf800,
    CLK3_DLY_OFF = 0x4,
    CLK3_DLY_MSK = 0x7f0,
    CLK3_PWR_OFF = 0x1,
    CLK3_PWR_MSK = 0xe,
    CLK3_EN_OFF = 0x0,
    CLK3_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R7(r7_undisclosed, clk3_dly, clk3_pwr, clk3_en) MAKE_LMX1205_REG_WR(R7, \
    (((r7_undisclosed) << R7_UNDISCLOSED_OFF) & R7_UNDISCLOSED_MSK) |  \
    (((clk3_dly) << CLK3_DLY_OFF) & CLK3_DLY_MSK) |  \
    (((clk3_pwr) << CLK3_PWR_OFF) & CLK3_PWR_MSK) |  \
    (((clk3_en) << CLK3_EN_OFF) & CLK3_EN_MSK))
// Register R8 [0x8] -- R8

enum r8_fields_t {
    R8_UNDISCLOSED_OFF = 0xf,
    R8_UNDISCLOSED_MSK = 0x8000,
    SYSREF0_PWR_LOW_OFF = 0xe,
    SYSREF0_PWR_LOW_MSK = 0x4000,
    SYSREF0_AC_OFF = 0xd,
    SYSREF0_AC_MSK = 0x2000,
    R8_UNDISCLOSED_0_OFF = 0xa,
    R8_UNDISCLOSED_0_MSK = 0x1c00,
    SYSREF0_VCM_OFF = 0x4,
    SYSREF0_VCM_MSK = 0x3f0,
    SYSREF0_PWR_OFF = 0x1,
    SYSREF0_PWR_MSK = 0xe,
    SYSREF0_EN_OFF = 0x0,
    SYSREF0_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R8(r8_undisclosed, sysref0_pwr_low, sysref0_ac, r8_undisclosed_0, sysref0_vcm, sysref0_pwr, sysref0_en) MAKE_LMX1205_REG_WR(R8, \
    (((r8_undisclosed) << R8_UNDISCLOSED_OFF) & R8_UNDISCLOSED_MSK) |  \
    (((sysref0_pwr_low) << SYSREF0_PWR_LOW_OFF) & SYSREF0_PWR_LOW_MSK) |  \
    (((sysref0_ac) << SYSREF0_AC_OFF) & SYSREF0_AC_MSK) |  \
    (((r8_undisclosed_0) << R8_UNDISCLOSED_0_OFF) & R8_UNDISCLOSED_0_MSK) |  \
    (((sysref0_vcm) << SYSREF0_VCM_OFF) & SYSREF0_VCM_MSK) |  \
    (((sysref0_pwr) << SYSREF0_PWR_OFF) & SYSREF0_PWR_MSK) |  \
    (((sysref0_en) << SYSREF0_EN_OFF) & SYSREF0_EN_MSK))
// Register R9 [0x9] -- R9

enum r9_fields_t {
    R9_UNDISCLOSED_OFF = 0xf,
    R9_UNDISCLOSED_MSK = 0x8000,
    SYSREF1_PWR_LOW_OFF = 0xe,
    SYSREF1_PWR_LOW_MSK = 0x4000,
    SYSREF1_AC_OFF = 0xd,
    SYSREF1_AC_MSK = 0x2000,
    R9_UNDISCLOSED_0_OFF = 0xa,
    R9_UNDISCLOSED_0_MSK = 0x1c00,
    SYSREF1_VCM_OFF = 0x4,
    SYSREF1_VCM_MSK = 0x3f0,
    SYSREF1_PWR_OFF = 0x1,
    SYSREF1_PWR_MSK = 0xe,
    SYSREF1_EN_OFF = 0x0,
    SYSREF1_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R9(r9_undisclosed, sysref1_pwr_low, sysref1_ac, r9_undisclosed_0, sysref1_vcm, sysref1_pwr, sysref1_en) MAKE_LMX1205_REG_WR(R9, \
    (((r9_undisclosed) << R9_UNDISCLOSED_OFF) & R9_UNDISCLOSED_MSK) |  \
    (((sysref1_pwr_low) << SYSREF1_PWR_LOW_OFF) & SYSREF1_PWR_LOW_MSK) |  \
    (((sysref1_ac) << SYSREF1_AC_OFF) & SYSREF1_AC_MSK) |  \
    (((r9_undisclosed_0) << R9_UNDISCLOSED_0_OFF) & R9_UNDISCLOSED_0_MSK) |  \
    (((sysref1_vcm) << SYSREF1_VCM_OFF) & SYSREF1_VCM_MSK) |  \
    (((sysref1_pwr) << SYSREF1_PWR_OFF) & SYSREF1_PWR_MSK) |  \
    (((sysref1_en) << SYSREF1_EN_OFF) & SYSREF1_EN_MSK))
// Register R10 [0xa] -- R10

enum r10_fields_t {
    R10_UNDISCLOSED_OFF = 0xf,
    R10_UNDISCLOSED_MSK = 0x8000,
    SYSREF2_PWR_LOW_OFF = 0xe,
    SYSREF2_PWR_LOW_MSK = 0x4000,
    SYSREF2_AC_OFF = 0xd,
    SYSREF2_AC_MSK = 0x2000,
    R10_UNDISCLOSED_0_OFF = 0xa,
    R10_UNDISCLOSED_0_MSK = 0x1c00,
    SYSREF2_VCM_OFF = 0x4,
    SYSREF2_VCM_MSK = 0x3f0,
    SYSREF2_PWR_OFF = 0x1,
    SYSREF2_PWR_MSK = 0xe,
    SYSREF2_EN_OFF = 0x0,
    SYSREF2_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R10(r10_undisclosed, sysref2_pwr_low, sysref2_ac, r10_undisclosed_0, sysref2_vcm, sysref2_pwr, sysref2_en) MAKE_LMX1205_REG_WR(R10, \
    (((r10_undisclosed) << R10_UNDISCLOSED_OFF) & R10_UNDISCLOSED_MSK) |  \
    (((sysref2_pwr_low) << SYSREF2_PWR_LOW_OFF) & SYSREF2_PWR_LOW_MSK) |  \
    (((sysref2_ac) << SYSREF2_AC_OFF) & SYSREF2_AC_MSK) |  \
    (((r10_undisclosed_0) << R10_UNDISCLOSED_0_OFF) & R10_UNDISCLOSED_0_MSK) |  \
    (((sysref2_vcm) << SYSREF2_VCM_OFF) & SYSREF2_VCM_MSK) |  \
    (((sysref2_pwr) << SYSREF2_PWR_OFF) & SYSREF2_PWR_MSK) |  \
    (((sysref2_en) << SYSREF2_EN_OFF) & SYSREF2_EN_MSK))
// Register R11 [0xb] -- R11

enum r11_fields_t {
    R11_UNDISCLOSED_OFF = 0xf,
    R11_UNDISCLOSED_MSK = 0x8000,
    SYSREF3_PWR_LOW_OFF = 0xe,
    SYSREF3_PWR_LOW_MSK = 0x4000,
    SYSREF3_AC_OFF = 0xd,
    SYSREF3_AC_MSK = 0x2000,
    R11_UNDISCLOSED_0_OFF = 0xa,
    R11_UNDISCLOSED_0_MSK = 0x1c00,
    SYSREF3_VCM_OFF = 0x4,
    SYSREF3_VCM_MSK = 0x3f0,
    SYSREF3_PWR_OFF = 0x1,
    SYSREF3_PWR_MSK = 0xe,
    SYSREF3_EN_OFF = 0x0,
    SYSREF3_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R11(r11_undisclosed, sysref3_pwr_low, sysref3_ac, r11_undisclosed_0, sysref3_vcm, sysref3_pwr, sysref3_en) MAKE_LMX1205_REG_WR(R11, \
    (((r11_undisclosed) << R11_UNDISCLOSED_OFF) & R11_UNDISCLOSED_MSK) |  \
    (((sysref3_pwr_low) << SYSREF3_PWR_LOW_OFF) & SYSREF3_PWR_LOW_MSK) |  \
    (((sysref3_ac) << SYSREF3_AC_OFF) & SYSREF3_AC_MSK) |  \
    (((r11_undisclosed_0) << R11_UNDISCLOSED_0_OFF) & R11_UNDISCLOSED_0_MSK) |  \
    (((sysref3_vcm) << SYSREF3_VCM_OFF) & SYSREF3_VCM_MSK) |  \
    (((sysref3_pwr) << SYSREF3_PWR_OFF) & SYSREF3_PWR_MSK) |  \
    (((sysref3_en) << SYSREF3_EN_OFF) & SYSREF3_EN_MSK))
// Register R12 [0xc] -- R12
enum logiclk_fmt_options {
    LOGICLK_FMT_LVDS = 0,
    LOGICLK_FMT_CML = 2,
};

enum r12_fields_t {
    R12_UNDISC_OFF = 0xd,
    R12_UNDISC_MSK = 0xe000,
    LOGICLK_FMT_OFF = 0xb,
    LOGICLK_FMT_MSK = 0x1800,
    R12_UNDISCLOSED_0_OFF = 0x9,
    R12_UNDISCLOSED_0_MSK = 0x600,
    LOGICLK_VCM_OFF = 0x4,
    LOGICLK_VCM_MSK = 0x1f0,
    LOGICLK_PWR_OFF = 0x1,
    LOGICLK_PWR_MSK = 0xe,
    LOGICLK_EN_OFF = 0x0,
    LOGICLK_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R12(r12_undisc, logiclk_fmt, r12_undisclosed_0, logiclk_vcm, logiclk_pwr, logiclk_en) MAKE_LMX1205_REG_WR(R12, \
    (((r12_undisc) << R12_UNDISC_OFF) & R12_UNDISC_MSK) |  \
    (((logiclk_fmt) << LOGICLK_FMT_OFF) & LOGICLK_FMT_MSK) |  \
    (((r12_undisclosed_0) << R12_UNDISCLOSED_0_OFF) & R12_UNDISCLOSED_0_MSK) |  \
    (((logiclk_vcm) << LOGICLK_VCM_OFF) & LOGICLK_VCM_MSK) |  \
    (((logiclk_pwr) << LOGICLK_PWR_OFF) & LOGICLK_PWR_MSK) |  \
    (((logiclk_en) << LOGICLK_EN_OFF) & LOGICLK_EN_MSK))
// Register R13 [0xd] -- R13
enum logisysref_fmt_options {
    LOGISYSREF_FMT_LVDS = 0,
    LOGISYSREF_FMT_CML = 2,
};

enum r13_fields_t {
    R13_UNDISCLOSED_OFF = 0xd,
    R13_UNDISCLOSED_MSK = 0xe000,
    LOGISYSREF_FMT_OFF = 0xb,
    LOGISYSREF_FMT_MSK = 0x1800,
    R13_UNDISCLOSED_0_OFF = 0x9,
    R13_UNDISCLOSED_0_MSK = 0x600,
    LOGISYSREF_VCM_OFF = 0x4,
    LOGISYSREF_VCM_MSK = 0x1f0,
    LOGISYSREF_PWR_OFF = 0x1,
    LOGISYSREF_PWR_MSK = 0xe,
    LOGISYSREF_EN_OFF = 0x0,
    LOGISYSREF_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R13(r13_undisclosed, logisysref_fmt, r13_undisclosed_0, logisysref_vcm, logisysref_pwr, logisysref_en) MAKE_LMX1205_REG_WR(R13, \
    (((r13_undisclosed) << R13_UNDISCLOSED_OFF) & R13_UNDISCLOSED_MSK) |  \
    (((logisysref_fmt) << LOGISYSREF_FMT_OFF) & LOGISYSREF_FMT_MSK) |  \
    (((r13_undisclosed_0) << R13_UNDISCLOSED_0_OFF) & R13_UNDISCLOSED_0_MSK) |  \
    (((logisysref_vcm) << LOGISYSREF_VCM_OFF) & LOGISYSREF_VCM_MSK) |  \
    (((logisysref_pwr) << LOGISYSREF_PWR_OFF) & LOGISYSREF_PWR_MSK) |  \
    (((logisysref_en) << LOGISYSREF_EN_OFF) & LOGISYSREF_EN_MSK))
// Register R14 [0xe] -- R14
enum logiclk_div_options {
    LOGICLK_DIV_DIV2 = 2,
    LOGICLK_DIV_DIV3 = 3,
    LOGICLK_DIV_DIV1023 = 1023,
};
enum logiclk_div_pre_options {
    LOGICLK_DIV_PRE_DIV1 = 1,
    LOGICLK_DIV_PRE_DIV2 = 2,
    LOGICLK_DIV_PRE_DIV4 = 4,
};

enum r14_fields_t {
    LOGICLK_DIV_RST_OFF = 0xf,
    LOGICLK_DIV_RST_MSK = 0x8000,
    R14_UNDISCLOSED_OFF = 0xd,
    R14_UNDISCLOSED_MSK = 0x6000,
    LOGICLK_DIV_OFF = 0x3,
    LOGICLK_DIV_MSK = 0x1ff8,
    LOGICLK_DIV_PRE_OFF = 0x0,
    LOGICLK_DIV_PRE_MSK = 0x7,
};
#define MAKE_LMX1205_R14(logiclk_div_rst, r14_undisclosed, logiclk_div, logiclk_div_pre) MAKE_LMX1205_REG_WR(R14, \
    (((logiclk_div_rst) << LOGICLK_DIV_RST_OFF) & LOGICLK_DIV_RST_MSK) |  \
    (((r14_undisclosed) << R14_UNDISCLOSED_OFF) & R14_UNDISCLOSED_MSK) |  \
    (((logiclk_div) << LOGICLK_DIV_OFF) & LOGICLK_DIV_MSK) |  \
    (((logiclk_div_pre) << LOGICLK_DIV_PRE_OFF) & LOGICLK_DIV_PRE_MSK))
// Register R15 [0xf] -- R15
enum logiclk2_en_options {
    LOGICLK2_EN_LOGISYSREFOUT = 0,
    LOGICLK2_EN_LOGICLKOUT1 = 1,
};

enum r15_fields_t {
    R15_UNDISCLOSED_OFF = 0x3,
    R15_UNDISCLOSED_MSK = 0xfff8,
    LOGICLK2_DIV_OFF = 0x1,
    LOGICLK2_DIV_MSK = 0x6,
    LOGICLK2_EN_OFF = 0x0,
    LOGICLK2_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R15(r15_undisclosed, logiclk2_div, logiclk2_en) MAKE_LMX1205_REG_WR(R15, \
    (((r15_undisclosed) << R15_UNDISCLOSED_OFF) & R15_UNDISCLOSED_MSK) |  \
    (((logiclk2_div) << LOGICLK2_DIV_OFF) & LOGICLK2_DIV_MSK) |  \
    (((logiclk2_en) << LOGICLK2_EN_OFF) & LOGICLK2_EN_MSK))
// Register R16 [0x10] -- R16
enum sysref_dly_scale_options {
    SYSREF_DLY_SCALE_400MHZ_TO_800MHZ = 0,
    SYSREF_DLY_SCALE_200MHZ_TO_400MHZ = 1,
    SYSREF_DLY_SCALE_150MHZ_TO_200MHZ = 2,
};
enum sysrefreq_dly_step_options {
    SYSREFREQ_DLY_STEP_28PS_1_4GHZ_TO_2_7GHZ = 0,
    SYSREFREQ_DLY_STEP_15PS__2_4GHZ_TO_4_7GHZ = 1,
    SYSREFREQ_DLY_STEP_11PS_3_1GHZ_TO_5_7GHZ = 2,
    SYSREFREQ_DLY_STEP_8PS_4_5GHZ_TO_12_8GHZ = 3,
};
enum sysrefreq_vcm_offset_options {
    SYSREFREQ_VCM_OFFSET_25MV = 0,
    SYSREFREQ_VCM_OFFSET_50MV = 1,
    SYSREFREQ_VCM_OFFSET_100MV = 2,
    SYSREFREQ_VCM_OFFSET_150MV = 3,
};
enum sysrefreq_vcm_options {
    SYSREFREQ_VCM_ZERO_OFFSET_AC_COUPLED = 0,
    SYSREFREQ_VCM_PIN_P_BIASED_HIGHER_THAN_PIN_N_AC_COUPLED = 1,
    SYSREFREQ_VCM_PIN_N_HIGHER_THAN_PIN_P_AC_COUPLED = 2,
    SYSREFREQ_VCM_NO_BIAS_DC_COUPLED = 3,
};

enum r16_fields_t {
    R16_UNDISCLOSED_OFF = 0x8,
    R16_UNDISCLOSED_MSK = 0xff00,
    SYSREF_DLY_SCALE_OFF = 0x6,
    SYSREF_DLY_SCALE_MSK = 0xc0,
    SYSREFREQ_DLY_STEP_OFF = 0x4,
    SYSREFREQ_DLY_STEP_MSK = 0x30,
    SYSREFREQ_VCM_OFFSET_OFF = 0x2,
    SYSREFREQ_VCM_OFFSET_MSK = 0xc,
    SYSREFREQ_VCM_OFF = 0x0,
    SYSREFREQ_VCM_MSK = 0x3,
};
#define MAKE_LMX1205_R16(r16_undisclosed, sysref_dly_scale, sysrefreq_dly_step, sysrefreq_vcm_offset, sysrefreq_vcm) MAKE_LMX1205_REG_WR(R16, \
    (((r16_undisclosed) << R16_UNDISCLOSED_OFF) & R16_UNDISCLOSED_MSK) |  \
    (((sysref_dly_scale) << SYSREF_DLY_SCALE_OFF) & SYSREF_DLY_SCALE_MSK) |  \
    (((sysrefreq_dly_step) << SYSREFREQ_DLY_STEP_OFF) & SYSREFREQ_DLY_STEP_MSK) |  \
    (((sysrefreq_vcm_offset) << SYSREFREQ_VCM_OFFSET_OFF) & SYSREFREQ_VCM_OFFSET_MSK) |  \
    (((sysrefreq_vcm) << SYSREFREQ_VCM_OFF) & SYSREFREQ_VCM_MSK))
// Register R17 [0x11] -- R17
enum sysrefreq_input_options {
    SYSREFREQ_INPUT_SYSREFREQ_PIN = 0,
    SYSREFREQ_INPUT_FORCE_LOW = 1,
    SYSREFREQ_INPUT_FORCE_HIGH = 3,
};
enum sysrefreq_mode_options {
    SYSREFREQ_MODE_SYNC = 0,
    SYSREFREQ_MODE_SYSREFREQ = 1,
    SYSREFREQ_MODE_SYSREF_WINDOWING = 2,
};

enum r17_fields_t {
    R17_UNDISCLOSED_OFF = 0xc,
    R17_UNDISCLOSED_MSK = 0xf000,
    R17_UNDISCLOSED_0_OFF = 0x8,
    R17_UNDISCLOSED_0_MSK = 0xf00,
    SYSREFREQ_INPUT_OFF = 0x6,
    SYSREFREQ_INPUT_MSK = 0xc0,
    SYSWND_UPDATE_STOP_OFF = 0x5,
    SYSWND_UPDATE_STOP_MSK = 0x20,
    SYNC_STOP_OFF = 0x4,
    SYNC_STOP_MSK = 0x10,
    SYSWND_LATCH_OFF = 0x3,
    SYSWND_LATCH_MSK = 0x8,
    SYSREFREQ_CLR_OFF = 0x2,
    SYSREFREQ_CLR_MSK = 0x4,
    SYSREFREQ_MODE_OFF = 0x0,
    SYSREFREQ_MODE_MSK = 0x3,
};
#define MAKE_LMX1205_R17(r17_undisclosed, r17_undisclosed_0, sysrefreq_input, syswnd_update_stop, sync_stop, syswnd_latch, sysrefreq_clr, sysrefreq_mode) MAKE_LMX1205_REG_WR(R17, \
    (((r17_undisclosed) << R17_UNDISCLOSED_OFF) & R17_UNDISCLOSED_MSK) |  \
    (((r17_undisclosed_0) << R17_UNDISCLOSED_0_OFF) & R17_UNDISCLOSED_0_MSK) |  \
    (((sysrefreq_input) << SYSREFREQ_INPUT_OFF) & SYSREFREQ_INPUT_MSK) |  \
    (((syswnd_update_stop) << SYSWND_UPDATE_STOP_OFF) & SYSWND_UPDATE_STOP_MSK) |  \
    (((sync_stop) << SYNC_STOP_OFF) & SYNC_STOP_MSK) |  \
    (((syswnd_latch) << SYSWND_LATCH_OFF) & SYSWND_LATCH_MSK) |  \
    (((sysrefreq_clr) << SYSREFREQ_CLR_OFF) & SYSREFREQ_CLR_MSK) |  \
    (((sysrefreq_mode) << SYSREFREQ_MODE_OFF) & SYSREFREQ_MODE_MSK))
// Register R18 [0x12] -- R18

enum r18_fields_t {
    R18_UNDISCLOSED_OFF = 0x6,
    R18_UNDISCLOSED_MSK = 0xffc0,
    SYSREFREQ_DLY_OFF = 0x0,
    SYSREFREQ_DLY_MSK = 0x3f,
};
#define MAKE_LMX1205_R18(r18_undisclosed, sysrefreq_dly) MAKE_LMX1205_REG_WR(R18, \
    (((r18_undisclosed) << R18_UNDISCLOSED_OFF) & R18_UNDISCLOSED_MSK) |  \
    (((sysrefreq_dly) << SYSREFREQ_DLY_OFF) & SYSREFREQ_DLY_MSK))
// Register R19 [0x13] -- R19
enum sysref_pulse_cnt_options {
    SYSREF_PULSE_CNT_1_PULSE = 1,
    SYSREF_PULSE_CNT_2_PULSES = 2,
    SYSREF_PULSE_CNT_15_PULSES = 15,
};
enum sysref_mode_options {
    SYSREF_MODE_CONTINUOUS = 0,
    SYSREF_MODE_PULSER = 1,
    SYSREF_MODE_REPEATER = 2,
    SYSREF_MODE_REPEATER_RETIME = 3,
};

enum r19_fields_t {
    R19_UNDISCLOSED_OFF = 0x7,
    R19_UNDISCLOSED_MSK = 0xff80,
    SYSREF_DLY_BYP_OFF = 0x6,
    SYSREF_DLY_BYP_MSK = 0x40,
    SYSREF_PULSE_CNT_OFF = 0x2,
    SYSREF_PULSE_CNT_MSK = 0x3c,
    SYSREF_MODE_OFF = 0x0,
    SYSREF_MODE_MSK = 0x3,
};
#define MAKE_LMX1205_R19(r19_undisclosed, sysref_dly_byp, sysref_pulse_cnt, sysref_mode) MAKE_LMX1205_REG_WR(R19, \
    (((r19_undisclosed) << R19_UNDISCLOSED_OFF) & R19_UNDISCLOSED_MSK) |  \
    (((sysref_dly_byp) << SYSREF_DLY_BYP_OFF) & SYSREF_DLY_BYP_MSK) |  \
    (((sysref_pulse_cnt) << SYSREF_PULSE_CNT_OFF) & SYSREF_PULSE_CNT_MSK) |  \
    (((sysref_mode) << SYSREF_MODE_OFF) & SYSREF_MODE_MSK))
// Register R20 [0x14] -- R20
enum sysref_dly_div_options {
    SYSREF_DLY_DIV_DIV2_LE_1_6GHZ = 0,
    SYSREF_DLY_DIV_DIV4_1_6GHZ_TO_3_2GHZ = 1,
    SYSREF_DLY_DIV_DIV8_3_2GHZ_TO_6_4GHZ = 2,
    SYSREF_DLY_DIV_DIV16_6_4GHZ_TO_12_8GHZ = 3,
};
enum sysref_div_options {
    SYSREF_DIV_DIV2 = 2,
    SYSREF_DIV_DIV3 = 3,
    SYSREF_DIV_DIV4095 = 4095,
};
enum sysref_div_pre_options {
    SYSREF_DIV_PRE_DIV1 = 0,
    SYSREF_DIV_PRE_DIV2 = 1,
    SYSREF_DIV_PRE_DIV4 = 2,
};

enum r20_fields_t {
    SYSREF_DLY_DIV_OFF = 0xe,
    SYSREF_DLY_DIV_MSK = 0xc000,
    SYSREF_DIV_OFF = 0x2,
    SYSREF_DIV_MSK = 0x3ffc,
    SYSREF_DIV_PRE_OFF = 0x0,
    SYSREF_DIV_PRE_MSK = 0x3,
};
#define MAKE_LMX1205_R20(sysref_dly_div, sysref_div, sysref_div_pre) MAKE_LMX1205_REG_WR(R20, \
    (((sysref_dly_div) << SYSREF_DLY_DIV_OFF) & SYSREF_DLY_DIV_MSK) |  \
    (((sysref_div) << SYSREF_DIV_OFF) & SYSREF_DIV_MSK) |  \
    (((sysref_div_pre) << SYSREF_DIV_PRE_OFF) & SYSREF_DIV_PRE_MSK))
// Register R21 [0x15] -- R21
enum sysref0_dly_phase_options {
    SYSREF0_DLY_PHASE_ICLKMARK = 0,
    SYSREF0_DLY_PHASE_QCLKMARK = 1,
    SYSREF0_DLY_PHASE_ICLK = 2,
    SYSREF0_DLY_PHASE_QCLK = 3,
};

enum r21_fields_t {
    R21_UNDISCLOSED_OFF = 0x9,
    R21_UNDISCLOSED_MSK = 0xfe00,
    SYSREF0_DLY_OFF = 0x2,
    SYSREF0_DLY_MSK = 0x1fc,
    SYSREF0_DLY_PHASE_OFF = 0x0,
    SYSREF0_DLY_PHASE_MSK = 0x3,
};
#define MAKE_LMX1205_R21(r21_undisclosed, sysref0_dly, sysref0_dly_phase) MAKE_LMX1205_REG_WR(R21, \
    (((r21_undisclosed) << R21_UNDISCLOSED_OFF) & R21_UNDISCLOSED_MSK) |  \
    (((sysref0_dly) << SYSREF0_DLY_OFF) & SYSREF0_DLY_MSK) |  \
    (((sysref0_dly_phase) << SYSREF0_DLY_PHASE_OFF) & SYSREF0_DLY_PHASE_MSK))
// Register R22 [0x16] -- R22
enum sysref1_dly_phase_options {
    SYSREF1_DLY_PHASE_ICLKMARK = 0,
    SYSREF1_DLY_PHASE_QCLKMARK = 1,
    SYSREF1_DLY_PHASE_QCLK = 2,
    SYSREF1_DLY_PHASE_ICLK = 3,
};

enum r22_fields_t {
    R22_UNDISCLOSED_OFF = 0x9,
    R22_UNDISCLOSED_MSK = 0xfe00,
    SYSREF1_DLY_OFF = 0x2,
    SYSREF1_DLY_MSK = 0x1fc,
    SYSREF1_DLY_PHASE_OFF = 0x0,
    SYSREF1_DLY_PHASE_MSK = 0x3,
};
#define MAKE_LMX1205_R22(r22_undisclosed, sysref1_dly, sysref1_dly_phase) MAKE_LMX1205_REG_WR(R22, \
    (((r22_undisclosed) << R22_UNDISCLOSED_OFF) & R22_UNDISCLOSED_MSK) |  \
    (((sysref1_dly) << SYSREF1_DLY_OFF) & SYSREF1_DLY_MSK) |  \
    (((sysref1_dly_phase) << SYSREF1_DLY_PHASE_OFF) & SYSREF1_DLY_PHASE_MSK))
// Register R23 [0x17] -- R23
enum sysref2_dly_phase_options {
    SYSREF2_DLY_PHASE_ICLKMARK = 0,
    SYSREF2_DLY_PHASE_QCLKMARK = 1,
    SYSREF2_DLY_PHASE_QCLK = 2,
    SYSREF2_DLY_PHASE_ICLK = 3,
};

enum r23_fields_t {
    R23_UNDISCLOSED_OFF = 0x9,
    R23_UNDISCLOSED_MSK = 0xfe00,
    SYSREF2_DLY_OFF = 0x2,
    SYSREF2_DLY_MSK = 0x1fc,
    SYSREF2_DLY_PHASE_OFF = 0x0,
    SYSREF2_DLY_PHASE_MSK = 0x3,
};
#define MAKE_LMX1205_R23(r23_undisclosed, sysref2_dly, sysref2_dly_phase) MAKE_LMX1205_REG_WR(R23, \
    (((r23_undisclosed) << R23_UNDISCLOSED_OFF) & R23_UNDISCLOSED_MSK) |  \
    (((sysref2_dly) << SYSREF2_DLY_OFF) & SYSREF2_DLY_MSK) |  \
    (((sysref2_dly_phase) << SYSREF2_DLY_PHASE_OFF) & SYSREF2_DLY_PHASE_MSK))
// Register R24 [0x18] -- R24
enum sysref3_dly_phase_options {
    SYSREF3_DLY_PHASE_ICLKMARK = 0,
    SYSREF3_DLY_PHASE_QCLKMARK = 1,
    SYSREF3_DLY_PHASE_QCLK = 2,
    SYSREF3_DLY_PHASE_ICLK = 3,
};

enum r24_fields_t {
    R24_UNDISCLOSED_OFF = 0x9,
    R24_UNDISCLOSED_MSK = 0xfe00,
    SYSREF3_DLY_OFF = 0x2,
    SYSREF3_DLY_MSK = 0x1fc,
    SYSREF3_DLY_PHASE_OFF = 0x0,
    SYSREF3_DLY_PHASE_MSK = 0x3,
};
#define MAKE_LMX1205_R24(r24_undisclosed, sysref3_dly, sysref3_dly_phase) MAKE_LMX1205_REG_WR(R24, \
    (((r24_undisclosed) << R24_UNDISCLOSED_OFF) & R24_UNDISCLOSED_MSK) |  \
    (((sysref3_dly) << SYSREF3_DLY_OFF) & SYSREF3_DLY_MSK) |  \
    (((sysref3_dly_phase) << SYSREF3_DLY_PHASE_OFF) & SYSREF3_DLY_PHASE_MSK))
// Register R25 [0x19] -- R25
enum logisysref_dly_options {
    LOGISYSREF_DLY_ICLKMARK = 0,
    LOGISYSREF_DLY_QCLKMARK = 1,
    LOGISYSREF_DLY_QCLK = 2,
    LOGISYSREF_DLY_ICLK = 3,
};

enum r25_fields_t {
    R25_UNDISCLOSED_OFF = 0x9,
    R25_UNDISCLOSED_MSK = 0xfe00,
    LOGISYSREF_DLY_OFF = 0x2,
    LOGISYSREF_DLY_MSK = 0x1fc,
};
#define MAKE_LMX1205_R25(r25_undisclosed, logisysref_dly) MAKE_LMX1205_REG_WR(R25, \
    (((r25_undisclosed) << R25_UNDISCLOSED_OFF) & R25_UNDISCLOSED_MSK) |  \
    (((logisysref_dly) << LOGISYSREF_DLY_OFF) & LOGISYSREF_DLY_MSK))
// Register R26 [0x1a] -- R26
enum smclk_div_options {
    SMCLK_DIV_DIV1 = 0,
    SMCLK_DIV_DIV2 = 1,
    SMCLK_DIV_DIV4 = 2,
    SMCLK_DIV_DIV8 = 3,
    SMCLK_DIV_DIV16 = 4,
    SMCLK_DIV_DIV32 = 5,
    SMCLK_DIV_DIV64 = 6,
    SMCLK_DIV_DIV128 = 7,
};
enum smclk_div_pre_options {
    SMCLK_DIV_PRE_DIV2 = 2,
    SMCLK_DIV_PRE_DIV4 = 4,
    SMCLK_DIV_PRE_DIV8 = 8,
};

enum r26_fields_t {
    R26_UNDISCLOSED_OFF = 0x8,
    R26_UNDISCLOSED_MSK = 0xff00,
    SMCLK_DIV_OFF = 0x5,
    SMCLK_DIV_MSK = 0xe0,
    SMCLK_DIV_PRE_OFF = 0x1,
    SMCLK_DIV_PRE_MSK = 0x1e,
    SMCLK_EN_OFF = 0x0,
    SMCLK_EN_MSK = 0x1,
};
#define MAKE_LMX1205_R26(r26_undisclosed, smclk_div, smclk_div_pre, smclk_en) MAKE_LMX1205_REG_WR(R26, \
    (((r26_undisclosed) << R26_UNDISCLOSED_OFF) & R26_UNDISCLOSED_MSK) |  \
    (((smclk_div) << SMCLK_DIV_OFF) & SMCLK_DIV_MSK) |  \
    (((smclk_div_pre) << SMCLK_DIV_PRE_OFF) & SMCLK_DIV_PRE_MSK) |  \
    (((smclk_en) << SMCLK_EN_OFF) & SMCLK_EN_MSK))
// Register R27 [0x1b] -- R27
enum clk_mux_options {
    CLK_MUX_RESERVED = 0,
    CLK_MUX_BUFFER = 1,
    CLK_MUX_DIVIDERS = 2,
    CLK_MUX_MULTIPLIER = 3,
};

enum r27_fields_t {
    R27_UNDISCLOSED_OFF = 0xc,
    R27_UNDISCLOSED_MSK = 0xf000,
    MULT_HIPFD_EN_OFF = 0xb,
    MULT_HIPFD_EN_MSK = 0x800,
    R27_UNDISCLOSED_0_OFF = 0xa,
    R27_UNDISCLOSED_0_MSK = 0x400,
    FCAL_EN_OFF = 0x9,
    FCAL_EN_MSK = 0x200,
    R27_UNDISCLOSED_1_OFF = 0x7,
    R27_UNDISCLOSED_1_MSK = 0x180,
    CLK_DIV_RST_OFF = 0x6,
    CLK_DIV_RST_MSK = 0x40,
    CLK_DIV_OFF = 0x3,
    CLK_DIV_MSK = 0x38,
    CLK_MUX_OFF = 0x0,
    CLK_MUX_MSK = 0x7,
};
#define MAKE_LMX1205_R27(r27_undisclosed, mult_hipfd_en, r27_undisclosed_0, fcal_en, r27_undisclosed_1, clk_div_rst, clk_div, clk_mux) MAKE_LMX1205_REG_WR(R27, \
    (((r27_undisclosed) << R27_UNDISCLOSED_OFF) & R27_UNDISCLOSED_MSK) |  \
    (((mult_hipfd_en) << MULT_HIPFD_EN_OFF) & MULT_HIPFD_EN_MSK) |  \
    (((r27_undisclosed_0) << R27_UNDISCLOSED_0_OFF) & R27_UNDISCLOSED_0_MSK) |  \
    (((fcal_en) << FCAL_EN_OFF) & FCAL_EN_MSK) |  \
    (((r27_undisclosed_1) << R27_UNDISCLOSED_1_OFF) & R27_UNDISCLOSED_1_MSK) |  \
    (((clk_div_rst) << CLK_DIV_RST_OFF) & CLK_DIV_RST_MSK) |  \
    (((clk_div) << CLK_DIV_OFF) & CLK_DIV_MSK) |  \
    (((clk_mux) << CLK_MUX_OFF) & CLK_MUX_MSK))
// Register R29 [0x1d] -- R29

enum r29_fields_t {
    RB_CLKPOS_U_OFF = 0x0,
    RB_CLKPOS_U_MSK = 0xffff,
};
#define MAKE_LMX1205_R29(rb_clkpos_u) MAKE_LMX1205_REG_WR(R29, \
    (((rb_clkpos_u) << RB_CLKPOS_U_OFF) & RB_CLKPOS_U_MSK))
// Register R30 [0x1e] -- R30

enum r30_fields_t {
    RB_CLKPOS_L_OFF = 0x0,
    RB_CLKPOS_L_MSK = 0xffff,
};
#define MAKE_LMX1205_R30(rb_clkpos_l) MAKE_LMX1205_REG_WR(R30, \
    (((rb_clkpos_l) << RB_CLKPOS_L_OFF) & RB_CLKPOS_L_MSK))
// Register R31 [0x1f] -- R31

enum r31_fields_t {
    R31_UNDISCLOSED_OFF = 0xe,
    R31_UNDISCLOSED_MSK = 0xc000,
    R31_UNDISCLOSED_0_OFF = 0xb,
    R31_UNDISCLOSED_0_MSK = 0x3800,
    RB_TEMPSENSE_OFF = 0x0,
    RB_TEMPSENSE_MSK = 0x7ff,
};
#define MAKE_LMX1205_R31(r31_undisclosed, r31_undisclosed_0, rb_tempsense) MAKE_LMX1205_REG_WR(R31, \
    (((r31_undisclosed) << R31_UNDISCLOSED_OFF) & R31_UNDISCLOSED_MSK) |  \
    (((r31_undisclosed_0) << R31_UNDISCLOSED_0_OFF) & R31_UNDISCLOSED_0_MSK) |  \
    (((rb_tempsense) << RB_TEMPSENSE_OFF) & RB_TEMPSENSE_MSK))
// Register R32 [0x20] -- R32

enum r32_fields_t {
    RB_VER_ID_OFF = 0x0,
    RB_VER_ID_MSK = 0xffff,
};
#define MAKE_LMX1205_R32(rb_ver_id) MAKE_LMX1205_REG_WR(R32, \
    (((rb_ver_id) << RB_VER_ID_OFF) & RB_VER_ID_MSK))
// Register R36 [0x24] -- R36

enum r36_fields_t {
    R36_UNDISCLOSED_OFF = 0xa,
    R36_UNDISCLOSED_MSK = 0xfc00,
    R36_UNDISCLOSED_0_OFF = 0x8,
    R36_UNDISCLOSED_0_MSK = 0x300,
    R36_UNDISCLOSED_1_OFF = 0x6,
    R36_UNDISCLOSED_1_MSK = 0xc0,
    R36_UNDISCLOSED_2_OFF = 0x0,
    R36_UNDISCLOSED_2_MSK = 0x3f,
};
#define MAKE_LMX1205_R36(r36_undisclosed, r36_undisclosed_0, r36_undisclosed_1, r36_undisclosed_2) MAKE_LMX1205_REG_WR(R36, \
    (((r36_undisclosed) << R36_UNDISCLOSED_OFF) & R36_UNDISCLOSED_MSK) |  \
    (((r36_undisclosed_0) << R36_UNDISCLOSED_0_OFF) & R36_UNDISCLOSED_0_MSK) |  \
    (((r36_undisclosed_1) << R36_UNDISCLOSED_1_OFF) & R36_UNDISCLOSED_1_MSK) |  \
    (((r36_undisclosed_2) << R36_UNDISCLOSED_2_OFF) & R36_UNDISCLOSED_2_MSK))
// Register R37 [0x25] -- R37
enum rb_lock_detect_options {
    RB_LOCK_DETECT_UNLOCK = 0,
    RB_LOCK_DETECT_LOCK_DETECT = 1,
};

enum r37_fields_t {
    R37_UNDISCLOSED_OFF = 0xf,
    R37_UNDISCLOSED_MSK = 0x8000,
    R37_UNDISCLOSED_0_OFF = 0x1,
    R37_UNDISCLOSED_0_MSK = 0x7ffe,
    RB_LOCK_DETECT_OFF = 0x0,
    RB_LOCK_DETECT_MSK = 0x1,
};
#define MAKE_LMX1205_R37(r37_undisclosed, r37_undisclosed_0, rb_lock_detect) MAKE_LMX1205_REG_WR(R37, \
    (((r37_undisclosed) << R37_UNDISCLOSED_OFF) & R37_UNDISCLOSED_MSK) |  \
    (((r37_undisclosed_0) << R37_UNDISCLOSED_0_OFF) & R37_UNDISCLOSED_0_MSK) |  \
    (((rb_lock_detect) << RB_LOCK_DETECT_OFF) & RB_LOCK_DETECT_MSK))
// Register R39 [0x27] -- R39

enum r39_fields_t {
    R39_UNDISCLOSED_OFF = 0xc,
    R39_UNDISCLOSED_MSK = 0xf000,
    R39_UNDISCLOSED_0_OFF = 0x9,
    R39_UNDISCLOSED_0_MSK = 0xe00,
    R39_UNDISCLOSED_1_OFF = 0x4,
    R39_UNDISCLOSED_1_MSK = 0x1f0,
    R39_UNDISCLOSED_2_OFF = 0x0,
    R39_UNDISCLOSED_2_MSK = 0xf,
};
#define MAKE_LMX1205_R39(r39_undisclosed, r39_undisclosed_0, r39_undisclosed_1, r39_undisclosed_2) MAKE_LMX1205_REG_WR(R39, \
    (((r39_undisclosed) << R39_UNDISCLOSED_OFF) & R39_UNDISCLOSED_MSK) |  \
    (((r39_undisclosed_0) << R39_UNDISCLOSED_0_OFF) & R39_UNDISCLOSED_0_MSK) |  \
    (((r39_undisclosed_1) << R39_UNDISCLOSED_1_OFF) & R39_UNDISCLOSED_1_MSK) |  \
    (((r39_undisclosed_2) << R39_UNDISCLOSED_2_OFF) & R39_UNDISCLOSED_2_MSK))
// Register R40 [0x28] -- R40

enum r40_fields_t {
    R40_UNDISCLOSED_OFF = 0xc,
    R40_UNDISCLOSED_MSK = 0xf000,
    R40_UNDISCLOSED_0_OFF = 0x9,
    R40_UNDISCLOSED_0_MSK = 0xe00,
    R40_UNDISCLOSED_1_OFF = 0x4,
    R40_UNDISCLOSED_1_MSK = 0x1f0,
    R40_UNDISCLOSED_2_OFF = 0x0,
    R40_UNDISCLOSED_2_MSK = 0xf,
};
#define MAKE_LMX1205_R40(r40_undisclosed, r40_undisclosed_0, r40_undisclosed_1, r40_undisclosed_2) MAKE_LMX1205_REG_WR(R40, \
    (((r40_undisclosed) << R40_UNDISCLOSED_OFF) & R40_UNDISCLOSED_MSK) |  \
    (((r40_undisclosed_0) << R40_UNDISCLOSED_0_OFF) & R40_UNDISCLOSED_0_MSK) |  \
    (((r40_undisclosed_1) << R40_UNDISCLOSED_1_OFF) & R40_UNDISCLOSED_1_MSK) |  \
    (((r40_undisclosed_2) << R40_UNDISCLOSED_2_OFF) & R40_UNDISCLOSED_2_MSK))
// Register R41 [0x29] -- R41

enum r41_fields_t {
    R41_UNDISCLOSED_OFF = 0xc,
    R41_UNDISCLOSED_MSK = 0xf000,
    R41_UNDISCLOSED_0_OFF = 0x9,
    R41_UNDISCLOSED_0_MSK = 0xe00,
    R41_UNDISCLOSED_1_OFF = 0x4,
    R41_UNDISCLOSED_1_MSK = 0x1f0,
    R41_UNDISCLOSED_2_OFF = 0x0,
    R41_UNDISCLOSED_2_MSK = 0xf,
};
#define MAKE_LMX1205_R41(r41_undisclosed, r41_undisclosed_0, r41_undisclosed_1, r41_undisclosed_2) MAKE_LMX1205_REG_WR(R41, \
    (((r41_undisclosed) << R41_UNDISCLOSED_OFF) & R41_UNDISCLOSED_MSK) |  \
    (((r41_undisclosed_0) << R41_UNDISCLOSED_0_OFF) & R41_UNDISCLOSED_0_MSK) |  \
    (((r41_undisclosed_1) << R41_UNDISCLOSED_1_OFF) & R41_UNDISCLOSED_1_MSK) |  \
    (((r41_undisclosed_2) << R41_UNDISCLOSED_2_OFF) & R41_UNDISCLOSED_2_MSK))
// Register R42 [0x2a] -- R42

enum r42_fields_t {
    R42_UNDISCLOSED_OFF = 0xc,
    R42_UNDISCLOSED_MSK = 0xf000,
    R42_UNDISCLOSED_0_OFF = 0x9,
    R42_UNDISCLOSED_0_MSK = 0xe00,
    R42_UNDISCLOSED_1_OFF = 0x4,
    R42_UNDISCLOSED_1_MSK = 0x1f0,
    R42_UNDISCLOSED_2_OFF = 0x0,
    R42_UNDISCLOSED_2_MSK = 0xf,
};
#define MAKE_LMX1205_R42(r42_undisclosed, r42_undisclosed_0, r42_undisclosed_1, r42_undisclosed_2) MAKE_LMX1205_REG_WR(R42, \
    (((r42_undisclosed) << R42_UNDISCLOSED_OFF) & R42_UNDISCLOSED_MSK) |  \
    (((r42_undisclosed_0) << R42_UNDISCLOSED_0_OFF) & R42_UNDISCLOSED_0_MSK) |  \
    (((r42_undisclosed_1) << R42_UNDISCLOSED_1_OFF) & R42_UNDISCLOSED_1_MSK) |  \
    (((r42_undisclosed_2) << R42_UNDISCLOSED_2_OFF) & R42_UNDISCLOSED_2_MSK))
// Register R43 [0x2b] -- R43

enum r43_fields_t {
    R43_UNDISCLOSED_OFF = 0xc,
    R43_UNDISCLOSED_MSK = 0xf000,
    R43_UNDISCLOSED_0_OFF = 0x9,
    R43_UNDISCLOSED_0_MSK = 0xe00,
    R43_UNDISCLOSED_1_OFF = 0x4,
    R43_UNDISCLOSED_1_MSK = 0x1f0,
    R43_UNDISCLOSED_2_OFF = 0x0,
    R43_UNDISCLOSED_2_MSK = 0xf,
};
#define MAKE_LMX1205_R43(r43_undisclosed, r43_undisclosed_0, r43_undisclosed_1, r43_undisclosed_2) MAKE_LMX1205_REG_WR(R43, \
    (((r43_undisclosed) << R43_UNDISCLOSED_OFF) & R43_UNDISCLOSED_MSK) |  \
    (((r43_undisclosed_0) << R43_UNDISCLOSED_0_OFF) & R43_UNDISCLOSED_0_MSK) |  \
    (((r43_undisclosed_1) << R43_UNDISCLOSED_1_OFF) & R43_UNDISCLOSED_1_MSK) |  \
    (((r43_undisclosed_2) << R43_UNDISCLOSED_2_OFF) & R43_UNDISCLOSED_2_MSK))
// Register R44 [0x2c] -- R44

enum r44_fields_t {
    R44_UNDISCLOSED_OFF = 0xc,
    R44_UNDISCLOSED_MSK = 0xf000,
    R44_UNDISCLOSED_0_OFF = 0x9,
    R44_UNDISCLOSED_0_MSK = 0xe00,
    R44_UNDISCLOSED_1_OFF = 0x4,
    R44_UNDISCLOSED_1_MSK = 0x1f0,
    R44_UNDISCLOSED_2_OFF = 0x0,
    R44_UNDISCLOSED_2_MSK = 0xf,
};
#define MAKE_LMX1205_R44(r44_undisclosed, r44_undisclosed_0, r44_undisclosed_1, r44_undisclosed_2) MAKE_LMX1205_REG_WR(R44, \
    (((r44_undisclosed) << R44_UNDISCLOSED_OFF) & R44_UNDISCLOSED_MSK) |  \
    (((r44_undisclosed_0) << R44_UNDISCLOSED_0_OFF) & R44_UNDISCLOSED_0_MSK) |  \
    (((r44_undisclosed_1) << R44_UNDISCLOSED_1_OFF) & R44_UNDISCLOSED_1_MSK) |  \
    (((r44_undisclosed_2) << R44_UNDISCLOSED_2_OFF) & R44_UNDISCLOSED_2_MSK))
// Register R45 [0x2d] -- R45

enum r45_fields_t {
    R45_UNDISCLOSED_OFF = 0xc,
    R45_UNDISCLOSED_MSK = 0xf000,
    R45_UNDISCLOSED_0_OFF = 0xa,
    R45_UNDISCLOSED_0_MSK = 0xc00,
    R45_UNDISCLOSED_1_OFF = 0x8,
    R45_UNDISCLOSED_1_MSK = 0x300,
    R45_UNDISCLOSED_2_OFF = 0x6,
    R45_UNDISCLOSED_2_MSK = 0xc0,
    R45_UNDISCLOSED_3_OFF = 0x4,
    R45_UNDISCLOSED_3_MSK = 0x30,
    R45_UNDISCLOSED_4_OFF = 0x2,
    R45_UNDISCLOSED_4_MSK = 0xc,
    R45_UNDISCLOSED_5_OFF = 0x0,
    R45_UNDISCLOSED_5_MSK = 0x3,
};
#define MAKE_LMX1205_R45(r45_undisclosed, r45_undisclosed_0, r45_undisclosed_1, r45_undisclosed_2, r45_undisclosed_3, r45_undisclosed_4, r45_undisclosed_5) MAKE_LMX1205_REG_WR(R45, \
    (((r45_undisclosed) << R45_UNDISCLOSED_OFF) & R45_UNDISCLOSED_MSK) |  \
    (((r45_undisclosed_0) << R45_UNDISCLOSED_0_OFF) & R45_UNDISCLOSED_0_MSK) |  \
    (((r45_undisclosed_1) << R45_UNDISCLOSED_1_OFF) & R45_UNDISCLOSED_1_MSK) |  \
    (((r45_undisclosed_2) << R45_UNDISCLOSED_2_OFF) & R45_UNDISCLOSED_2_MSK) |  \
    (((r45_undisclosed_3) << R45_UNDISCLOSED_3_OFF) & R45_UNDISCLOSED_3_MSK) |  \
    (((r45_undisclosed_4) << R45_UNDISCLOSED_4_OFF) & R45_UNDISCLOSED_4_MSK) |  \
    (((r45_undisclosed_5) << R45_UNDISCLOSED_5_OFF) & R45_UNDISCLOSED_5_MSK))
// Register R54 [0x36] -- R54

enum r54_fields_t {
    R54_UNDISCLOSED_OFF = 0xe,
    R54_UNDISCLOSED_MSK = 0xc000,
    R54_UNDISCLOSED_0_OFF = 0x4,
    R54_UNDISCLOSED_0_MSK = 0x3ff0,
    R54_UNDISCLOSED_1_OFF = 0x2,
    R54_UNDISCLOSED_1_MSK = 0xc,
    R54_UNDISCLOSED_2_OFF = 0x0,
    R54_UNDISCLOSED_2_MSK = 0x3,
};
#define MAKE_LMX1205_R54(r54_undisclosed, r54_undisclosed_0, r54_undisclosed_1, r54_undisclosed_2) MAKE_LMX1205_REG_WR(R54, \
    (((r54_undisclosed) << R54_UNDISCLOSED_OFF) & R54_UNDISCLOSED_MSK) |  \
    (((r54_undisclosed_0) << R54_UNDISCLOSED_0_OFF) & R54_UNDISCLOSED_0_MSK) |  \
    (((r54_undisclosed_1) << R54_UNDISCLOSED_1_OFF) & R54_UNDISCLOSED_1_MSK) |  \
    (((r54_undisclosed_2) << R54_UNDISCLOSED_2_OFF) & R54_UNDISCLOSED_2_MSK))
// Register R55 [0x37] -- R55

enum r55_fields_t {
    R55_UNDISCLOSED_OFF = 0x6,
    R55_UNDISCLOSED_MSK = 0xffc0,
    DEV_IOPT_CTRL_OFF = 0x0,
    DEV_IOPT_CTRL_MSK = 0x3f,
};
#define MAKE_LMX1205_R55(r55_undisclosed, dev_iopt_ctrl) MAKE_LMX1205_REG_WR(R55, \
    (((r55_undisclosed) << R55_UNDISCLOSED_OFF) & R55_UNDISCLOSED_MSK) |  \
    (((dev_iopt_ctrl) << DEV_IOPT_CTRL_OFF) & DEV_IOPT_CTRL_MSK))
// Register R77 [0x4d] -- R77

enum r77_fields_t {
    R77_UNDISCLOSED_OFF = 0x2,
    R77_UNDISCLOSED_MSK = 0xfffc,
    R77_UNDISCLOSED_0_OFF = 0x0,
    R77_UNDISCLOSED_0_MSK = 0x3,
};
#define MAKE_LMX1205_R77(r77_undisclosed, r77_undisclosed_0) MAKE_LMX1205_REG_WR(R77, \
    (((r77_undisclosed) << R77_UNDISCLOSED_OFF) & R77_UNDISCLOSED_MSK) |  \
    (((r77_undisclosed_0) << R77_UNDISCLOSED_0_OFF) & R77_UNDISCLOSED_0_MSK))
