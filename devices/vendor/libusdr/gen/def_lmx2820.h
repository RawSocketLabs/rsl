enum lmx2820_regs_t {
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
    R28 = 0x1c,
    R29 = 0x1d,
    R30 = 0x1e,
    R31 = 0x1f,
    R32 = 0x20,
    R33 = 0x21,
    R34 = 0x22,
    R35 = 0x23,
    R36 = 0x24,
    R37 = 0x25,
    R38 = 0x26,
    R39 = 0x27,
    R40 = 0x28,
    R41 = 0x29,
    R42 = 0x2a,
    R43 = 0x2b,
    R44 = 0x2c,
    R45 = 0x2d,
    R46 = 0x2e,
    R47 = 0x2f,
    R48 = 0x30,
    R49 = 0x31,
    R50 = 0x32,
    R51 = 0x33,
    R52 = 0x34,
    R53 = 0x35,
    R54 = 0x36,
    R55 = 0x37,
    R56 = 0x38,
    R57 = 0x39,
    R58 = 0x3a,
    R59 = 0x3b,
    R60 = 0x3c,
    R61 = 0x3d,
    R62 = 0x3e,
    R63 = 0x3f,
    R64 = 0x40,
    R65 = 0x41,
    R66 = 0x42,
    R67 = 0x43,
    R68 = 0x44,
    R69 = 0x45,
    R70 = 0x46,
    R71 = 0x47,
    R72 = 0x48,
    R73 = 0x49,
    R74 = 0x4a,
    R75 = 0x4b,
    R76 = 0x4c,
    R77 = 0x4d,
    R78 = 0x4e,
    R79 = 0x4f,
    R80 = 0x50,
    R81 = 0x51,
    R82 = 0x52,
    R83 = 0x53,
    R84 = 0x54,
    R85 = 0x55,
    R86 = 0x56,
    R87 = 0x57,
    R88 = 0x58,
    R89 = 0x59,
    R90 = 0x5a,
    R91 = 0x5b,
    R92 = 0x5c,
    R93 = 0x5d,
    R94 = 0x5e,
    R95 = 0x5f,
    R96 = 0x60,
    R97 = 0x61,
    R98 = 0x62,
    R99 = 0x63,
    R100 = 0x64,
    R101 = 0x65,
    R102 = 0x66,
    R103 = 0x67,
    R104 = 0x68,
    R105 = 0x69,
    R106 = 0x6a,
    R107 = 0x6b,
    R108 = 0x6c,
    R109 = 0x6d,
    R110 = 0x6e,
    R111 = 0x6f,
    R112 = 0x70,
    R113 = 0x71,
    R114 = 0x72,
    R115 = 0x73,
    R116 = 0x74,
    R117 = 0x75,
    R118 = 0x76,
    R119 = 0x77,
    R120 = 0x78,
    R121 = 0x79,
    R122 = 0x7a,
};
#define MAKE_LMX2820_REG_WR(a, v) (((a) << 16) | ((v) & 0xffff))
#define MAKE_LMX2820_REG_RD(a) (0x800000 | ((a) << 16))
// Register R0 [0x0] -- R0
enum fcal_hpfd_adj_options {
    FCAL_HPFD_ADJ_FPD_LE_100_MHZ = 0,
    FCAL_HPFD_ADJ_100_MHZ_LT_FPD_LE_150_MHZ = 1,
    FCAL_HPFD_ADJ_150_MHZ_LT_FPD_LE_200_MHZ = 2,
    FCAL_HPFD_ADJ_FPD_GT_200_MHZ = 3,
};
enum fcal_lpfd_adj_options {
    FCAL_LPFD_ADJ_FPD_GE_10_MHZ = 0,
    FCAL_LPFD_ADJ_10_MHZ_GT_FPD_GE_5_MHZ = 1,
    FCAL_LPFD_ADJ_5_MHZ_GT_FPD_GE_2_5_MHZ = 2,
    FCAL_LPFD_ADJ_FPD_LT_2_5_MHZ = 3,
};
enum dblr_cal_en_options {
    DBLR_CAL_EN_DISABLED = 0,
    DBLR_CAL_EN_ENABLED = 1,
};
enum fcal_en_options {
    FCAL_EN_DISABLED = 0,
    FCAL_EN_ENABLED = 1,
};
enum reset_options {
    RESET_NORMAL_OPERATION = 0,
    RESET_RESET = 1,
};
enum powerdown_options {
    POWERDOWN_NORMAL_OPERATION = 0,
    POWERDOWN_POWER_DOWN = 1,
};

enum r0_fields_t {
    R0_RESERVED_0_OFF = 0xe,
    R0_RESERVED_0_MSK = 0xc000,
    INSTCAL_SKIP_ACAL_OFF = 0xd,
    INSTCAL_SKIP_ACAL_MSK = 0x2000,
    R0_RESERVED_1_OFF = 0xb,
    R0_RESERVED_1_MSK = 0x1800,
    FCAL_HPFD_ADJ_OFF = 0x9,
    FCAL_HPFD_ADJ_MSK = 0x600,
    FCAL_LPFD_ADJ_OFF = 0x7,
    FCAL_LPFD_ADJ_MSK = 0x180,
    DBLR_CAL_EN_OFF = 0x6,
    DBLR_CAL_EN_MSK = 0x40,
    R0_RESERVED_2_OFF = 0x5,
    R0_RESERVED_2_MSK = 0x20,
    FCAL_EN_OFF = 0x4,
    FCAL_EN_MSK = 0x10,
    R0_RESERVED_3_OFF = 0x2,
    R0_RESERVED_3_MSK = 0xc,
    RESET_OFF = 0x1,
    RESET_MSK = 0x2,
    POWERDOWN_OFF = 0x0,
    POWERDOWN_MSK = 0x1,
};
#define MAKE_LMX2820_R0(r0_reserved_0, instcal_skip_acal, r0_reserved_1, fcal_hpfd_adj, fcal_lpfd_adj, dblr_cal_en, r0_reserved_2, fcal_en, r0_reserved_3, reset, powerdown) MAKE_LMX2820_REG_WR(R0, \
    (((r0_reserved_0) << R0_RESERVED_0_OFF) & R0_RESERVED_0_MSK) |  \
    (((instcal_skip_acal) << INSTCAL_SKIP_ACAL_OFF) & INSTCAL_SKIP_ACAL_MSK) |  \
    (((r0_reserved_1) << R0_RESERVED_1_OFF) & R0_RESERVED_1_MSK) |  \
    (((fcal_hpfd_adj) << FCAL_HPFD_ADJ_OFF) & FCAL_HPFD_ADJ_MSK) |  \
    (((fcal_lpfd_adj) << FCAL_LPFD_ADJ_OFF) & FCAL_LPFD_ADJ_MSK) |  \
    (((dblr_cal_en) << DBLR_CAL_EN_OFF) & DBLR_CAL_EN_MSK) |  \
    (((r0_reserved_2) << R0_RESERVED_2_OFF) & R0_RESERVED_2_MSK) |  \
    (((fcal_en) << FCAL_EN_OFF) & FCAL_EN_MSK) |  \
    (((r0_reserved_3) << R0_RESERVED_3_OFF) & R0_RESERVED_3_MSK) |  \
    (((reset) << RESET_OFF) & RESET_MSK) |  \
    (((powerdown) << POWERDOWN_OFF) & POWERDOWN_MSK))
// Register R1 [0x1] -- R1
enum phase_sync_en_options {
    PHASE_SYNC_EN_NORMAL_OPERATION = 0,
    PHASE_SYNC_EN_PHASE_SYNCHRONIZATION_ENABLED = 1,
};
enum ld_vtune_en_options {
    LD_VTUNE_EN_VCOCAL_LOCK_DETECT = 0,
    LD_VTUNE_EN_VCOCAL_AND_VTUNE_LOCK_DETECT = 1,
};
enum instcal_dblr_en_options {
    INSTCAL_DBLR_EN_NORMAL_OPERATION = 0,
    INSTCAL_DBLR_EN_VCO_DOUBLER_IS_ENGAGED = 1,
};
enum instcal_en_options {
    INSTCAL_EN_DISABLED = 0,
    INSTCAL_EN_ENABLED = 1,
};

enum r1_fields_t {
    PHASE_SYNC_EN_OFF = 0xf,
    PHASE_SYNC_EN_MSK = 0x8000,
    R1_RESERVED_0_OFF = 0x6,
    R1_RESERVED_0_MSK = 0x7fc0,
    LD_VTUNE_EN_OFF = 0x5,
    LD_VTUNE_EN_MSK = 0x20,
    R1_RESERVED_1_OFF = 0x2,
    R1_RESERVED_1_MSK = 0x1c,
    INSTCAL_DBLR_EN_OFF = 0x1,
    INSTCAL_DBLR_EN_MSK = 0x2,
    INSTCAL_EN_OFF = 0x0,
    INSTCAL_EN_MSK = 0x1,
};
#define MAKE_LMX2820_R1(phase_sync_en, r1_reserved_0, ld_vtune_en, r1_reserved_1, instcal_dblr_en, instcal_en) MAKE_LMX2820_REG_WR(R1, \
    (((phase_sync_en) << PHASE_SYNC_EN_OFF) & PHASE_SYNC_EN_MSK) |  \
    (((r1_reserved_0) << R1_RESERVED_0_OFF) & R1_RESERVED_0_MSK) |  \
    (((ld_vtune_en) << LD_VTUNE_EN_OFF) & LD_VTUNE_EN_MSK) |  \
    (((r1_reserved_1) << R1_RESERVED_1_OFF) & R1_RESERVED_1_MSK) |  \
    (((instcal_dblr_en) << INSTCAL_DBLR_EN_OFF) & INSTCAL_DBLR_EN_MSK) |  \
    (((instcal_en) << INSTCAL_EN_OFF) & INSTCAL_EN_MSK))
// Register R2 [0x2] -- R2
enum cal_clk_div_options {
    CAL_CLK_DIV_FOSCIN_LE_200_MHZ = 0,
    CAL_CLK_DIV_FOSCIN_LE_400_MHZ = 1,
    CAL_CLK_DIV_FOSCIN_LE_800_MHZ = 2,
    CAL_CLK_DIV_ALL_OTHER_FOSCIN_VALUES = 3,
};
enum quick_recal_en_options {
    QUICK_RECAL_EN_DISABLED = 0,
    QUICK_RECAL_EN_ENABLED = 1,
};

enum r2_fields_t {
    R2_RESERVED_0_OFF = 0xf,
    R2_RESERVED_0_MSK = 0x8000,
    CAL_CLK_DIV_OFF = 0xc,
    CAL_CLK_DIV_MSK = 0x7000,
    INSTCAL_DLY_OFF = 0x1,
    INSTCAL_DLY_MSK = 0xffe,
    QUICK_RECAL_EN_OFF = 0x0,
    QUICK_RECAL_EN_MSK = 0x1,
};
#define MAKE_LMX2820_R2(r2_reserved_0, cal_clk_div, instcal_dly, quick_recal_en) MAKE_LMX2820_REG_WR(R2, \
    (((r2_reserved_0) << R2_RESERVED_0_OFF) & R2_RESERVED_0_MSK) |  \
    (((cal_clk_div) << CAL_CLK_DIV_OFF) & CAL_CLK_DIV_MSK) |  \
    (((instcal_dly) << INSTCAL_DLY_OFF) & INSTCAL_DLY_MSK) |  \
    (((quick_recal_en) << QUICK_RECAL_EN_OFF) & QUICK_RECAL_EN_MSK))
// Register R3 [0x3] -- R3

enum r3_fields_t {
    R3_RESERVED_0_OFF = 0x0,
    R3_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R3(r3_reserved_0) MAKE_LMX2820_REG_WR(R3, \
    (((r3_reserved_0) << R3_RESERVED_0_OFF) & R3_RESERVED_0_MSK))
// Register R4 [0x4] -- R4

enum r4_fields_t {
    R4_RESERVED_0_OFF = 0x0,
    R4_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R4(r4_reserved_0) MAKE_LMX2820_REG_WR(R4, \
    (((r4_reserved_0) << R4_RESERVED_0_OFF) & R4_RESERVED_0_MSK))
// Register R5 [0x5] -- R5

enum r5_fields_t {
    R5_RESERVED_0_OFF = 0x0,
    R5_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R5(r5_reserved_0) MAKE_LMX2820_REG_WR(R5, \
    (((r5_reserved_0) << R5_RESERVED_0_OFF) & R5_RESERVED_0_MSK))
// Register R6 [0x6] -- R6

enum r6_fields_t {
    ACAL_CMP_DLY_OFF = 0x8,
    ACAL_CMP_DLY_MSK = 0xff00,
    R6_RESERVED_0_OFF = 0x0,
    R6_RESERVED_0_MSK = 0xff,
};
#define MAKE_LMX2820_R6(acal_cmp_dly, r6_reserved_0) MAKE_LMX2820_REG_WR(R6, \
    (((acal_cmp_dly) << ACAL_CMP_DLY_OFF) & ACAL_CMP_DLY_MSK) |  \
    (((r6_reserved_0) << R6_RESERVED_0_OFF) & R6_RESERVED_0_MSK))
// Register R7 [0x7] -- R7

enum r7_fields_t {
    R7_RESERVED_0_OFF = 0x0,
    R7_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R7(r7_reserved_0) MAKE_LMX2820_REG_WR(R7, \
    (((r7_reserved_0) << R7_RESERVED_0_OFF) & R7_RESERVED_0_MSK))
// Register R8 [0x8] -- R8

enum r8_fields_t {
    R8_RESERVED_0_OFF = 0x0,
    R8_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R8(r8_reserved_0) MAKE_LMX2820_REG_WR(R8, \
    (((r8_reserved_0) << R8_RESERVED_0_OFF) & R8_RESERVED_0_MSK))
// Register R9 [0x9] -- R9

enum r9_fields_t {
    R9_RESERVED_0_OFF = 0x0,
    R9_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R9(r9_reserved_0) MAKE_LMX2820_REG_WR(R9, \
    (((r9_reserved_0) << R9_RESERVED_0_OFF) & R9_RESERVED_0_MSK))
// Register R10 [0xa] -- R10
enum pfd_dly_manual_options {
    PFD_DLY_MANUAL_DISABLED = 0,
    PFD_DLY_MANUAL_ENABLED = 1,
};
enum vco_daciset_force_options {
    VCO_DACISET_FORCE_DISABLED = 0,
    VCO_DACISET_FORCE_ENABLED = 1,
};
enum vco_capctrl_force_options {
    VCO_CAPCTRL_FORCE_DISABLED = 0,
    VCO_CAPCTRL_FORCE_ENABLED = 1,
};

enum r10_fields_t {
    R10_RESERVED_0_OFF = 0xd,
    R10_RESERVED_0_MSK = 0xe000,
    PFD_DLY_MANUAL_OFF = 0xc,
    PFD_DLY_MANUAL_MSK = 0x1000,
    VCO_DACISET_FORCE_OFF = 0xb,
    VCO_DACISET_FORCE_MSK = 0x800,
    R10_RESERVED_1_OFF = 0x8,
    R10_RESERVED_1_MSK = 0x700,
    VCO_CAPCTRL_FORCE_OFF = 0x7,
    VCO_CAPCTRL_FORCE_MSK = 0x80,
    R10_RESERVED_2_OFF = 0x0,
    R10_RESERVED_2_MSK = 0x7f,
};
#define MAKE_LMX2820_R10(r10_reserved_0, pfd_dly_manual, vco_daciset_force, r10_reserved_1, vco_capctrl_force, r10_reserved_2) MAKE_LMX2820_REG_WR(R10, \
    (((r10_reserved_0) << R10_RESERVED_0_OFF) & R10_RESERVED_0_MSK) |  \
    (((pfd_dly_manual) << PFD_DLY_MANUAL_OFF) & PFD_DLY_MANUAL_MSK) |  \
    (((vco_daciset_force) << VCO_DACISET_FORCE_OFF) & VCO_DACISET_FORCE_MSK) |  \
    (((r10_reserved_1) << R10_RESERVED_1_OFF) & R10_RESERVED_1_MSK) |  \
    (((vco_capctrl_force) << VCO_CAPCTRL_FORCE_OFF) & VCO_CAPCTRL_FORCE_MSK) |  \
    (((r10_reserved_2) << R10_RESERVED_2_OFF) & R10_RESERVED_2_MSK))
// Register R11 [0xb] -- R11
enum osc_2x_options {
    OSC_2X_DISABLED = 0,
    OSC_2X_ENABLED = 1,
};

enum r11_fields_t {
    R11_RESERVED_0_OFF = 0x5,
    R11_RESERVED_0_MSK = 0xffe0,
    OSC_2X_OFF = 0x4,
    OSC_2X_MSK = 0x10,
    R11_RESERVED_1_OFF = 0x0,
    R11_RESERVED_1_MSK = 0xf,
};
#define MAKE_LMX2820_R11(r11_reserved_0, osc_2x, r11_reserved_1) MAKE_LMX2820_REG_WR(R11, \
    (((r11_reserved_0) << R11_RESERVED_0_OFF) & R11_RESERVED_0_MSK) |  \
    (((osc_2x) << OSC_2X_OFF) & OSC_2X_MSK) |  \
    (((r11_reserved_1) << R11_RESERVED_1_OFF) & R11_RESERVED_1_MSK))
// Register R12 [0xc] -- R12
enum mult_options {
    MULT_BYPASSED = 1,
    MULT_X3 = 3,
    MULT_X4 = 4,
    MULT_X5 = 5,
    MULT_X6 = 6,
    MULT_X7 = 7,
};

enum r12_fields_t {
    R12_RESERVED_0_OFF = 0xd,
    R12_RESERVED_0_MSK = 0xe000,
    MULT_OFF = 0xa,
    MULT_MSK = 0x1c00,
    R12_RESERVED_1_OFF = 0x0,
    R12_RESERVED_1_MSK = 0x3ff,
};
#define MAKE_LMX2820_R12(r12_reserved_0, mult, r12_reserved_1) MAKE_LMX2820_REG_WR(R12, \
    (((r12_reserved_0) << R12_RESERVED_0_OFF) & R12_RESERVED_0_MSK) |  \
    (((mult) << MULT_OFF) & MULT_MSK) |  \
    (((r12_reserved_1) << R12_RESERVED_1_OFF) & R12_RESERVED_1_MSK))
// Register R13 [0xd] -- R13

enum r13_fields_t {
    R13_RESERVED_0_OFF = 0xd,
    R13_RESERVED_0_MSK = 0xe000,
    PLL_R_OFF = 0x5,
    PLL_R_MSK = 0x1fe0,
    R13_RESERVED_1_OFF = 0x0,
    R13_RESERVED_1_MSK = 0x1f,
};
#define MAKE_LMX2820_R13(r13_reserved_0, pll_r, r13_reserved_1) MAKE_LMX2820_REG_WR(R13, \
    (((r13_reserved_0) << R13_RESERVED_0_OFF) & R13_RESERVED_0_MSK) |  \
    (((pll_r) << PLL_R_OFF) & PLL_R_MSK) |  \
    (((r13_reserved_1) << R13_RESERVED_1_OFF) & R13_RESERVED_1_MSK))
// Register R14 [0xe] -- R14

enum r14_fields_t {
    R14_RESERVED_0_OFF = 0xc,
    R14_RESERVED_0_MSK = 0xf000,
    PLL_R_PRE_OFF = 0x0,
    PLL_R_PRE_MSK = 0xfff,
};
#define MAKE_LMX2820_R14(r14_reserved_0, pll_r_pre) MAKE_LMX2820_REG_WR(R14, \
    (((r14_reserved_0) << R14_RESERVED_0_OFF) & R14_RESERVED_0_MSK) |  \
    (((pll_r_pre) << PLL_R_PRE_OFF) & PLL_R_PRE_MSK))
// Register R15 [0xf] -- R15
enum pfd_pol_options {
    PFD_POL_NEGATIVE_VTUNE = 0,
    PFD_POL_POSITIVE_VTUNE = 1,
};
enum pfd_single_options {
    PFD_SINGLE_NORMAL_OPERATION = 0,
    PFD_SINGLE_SINGLE_PFD = 3,
};

enum r15_fields_t {
    R15_RESERVED_0_OFF = 0xc,
    R15_RESERVED_0_MSK = 0xf000,
    PFD_POL_OFF = 0xb,
    PFD_POL_MSK = 0x800,
    PFD_SINGLE_OFF = 0x9,
    PFD_SINGLE_MSK = 0x600,
    R15_RESERVED_1_OFF = 0x0,
    R15_RESERVED_1_MSK = 0x1ff,
};
#define MAKE_LMX2820_R15(r15_reserved_0, pfd_pol, pfd_single, r15_reserved_1) MAKE_LMX2820_REG_WR(R15, \
    (((r15_reserved_0) << R15_RESERVED_0_OFF) & R15_RESERVED_0_MSK) |  \
    (((pfd_pol) << PFD_POL_OFF) & PFD_POL_MSK) |  \
    (((pfd_single) << PFD_SINGLE_OFF) & PFD_SINGLE_MSK) |  \
    (((r15_reserved_1) << R15_RESERVED_1_OFF) & R15_RESERVED_1_MSK))
// Register R16 [0x10] -- R16
enum cpg_options {
    CPG_TRI_STATE = 0,
    CPG_1_4_MA = 1,
    CPG_5_6_MA = 4,
    CPG_7_MA = 5,
    CPG_11_2_MA = 6,
    CPG_12_6_MA = 7,
    CPG_2_8_MA = 8,
    CPG_4_2_MA = 9,
    CPG_8_4_MA = 12,
    CPG_9_8_MA = 13,
    CPG_14_MA = 14,
    CPG_15_4_MA = 15,
};

enum r16_fields_t {
    R16_RESERVED_0_OFF = 0x5,
    R16_RESERVED_0_MSK = 0xffe0,
    CPG_OFF = 0x1,
    CPG_MSK = 0x1e,
    R16_RESERVED_1_OFF = 0x0,
    R16_RESERVED_1_MSK = 0x1,
};
#define MAKE_LMX2820_R16(r16_reserved_0, cpg, r16_reserved_1) MAKE_LMX2820_REG_WR(R16, \
    (((r16_reserved_0) << R16_RESERVED_0_OFF) & R16_RESERVED_0_MSK) |  \
    (((cpg) << CPG_OFF) & CPG_MSK) |  \
    (((r16_reserved_1) << R16_RESERVED_1_OFF) & R16_RESERVED_1_MSK))
// Register R17 [0x11] -- R17
enum ld_type_options {
    LD_TYPE_ONE_SHOT = 0,
    LD_TYPE_CONTINUOUS = 1,
};

enum r17_fields_t {
    R17_RESERVED_1_OFF = 0x7,
    R17_RESERVED_1_MSK = 0xff80,
    LD_TYPE_OFF = 0x6,
    LD_TYPE_MSK = 0x40,
    R17_RESERVED_0_OFF = 0x0,
    R17_RESERVED_0_MSK = 0x3f,
};
#define MAKE_LMX2820_R17(r17_reserved_1, ld_type, r17_reserved_0) MAKE_LMX2820_REG_WR(R17, \
    (((r17_reserved_1) << R17_RESERVED_1_OFF) & R17_RESERVED_1_MSK) |  \
    (((ld_type) << LD_TYPE_OFF) & LD_TYPE_MSK) |  \
    (((r17_reserved_0) << R17_RESERVED_0_OFF) & R17_RESERVED_0_MSK))
// Register R18 [0x12] -- R18

enum r18_fields_t {
    LD_DLY_OFF = 0x0,
    LD_DLY_MSK = 0xffff,
};
#define MAKE_LMX2820_R18(ld_dly) MAKE_LMX2820_REG_WR(R18, \
    (((ld_dly) << LD_DLY_OFF) & LD_DLY_MSK))
// Register R19 [0x13] -- R19
enum tempsense_en_options {
    TEMPSENSE_EN_DISABLED = 0,
    TEMPSENSE_EN_ENABLED = 3,
};

enum r19_fields_t {
    R19_RESERVED_0_OFF = 0x5,
    R19_RESERVED_0_MSK = 0xffe0,
    TEMPSENSE_EN_OFF = 0x3,
    TEMPSENSE_EN_MSK = 0x18,
    R19_RESERVED_1_OFF = 0x0,
    R19_RESERVED_1_MSK = 0x7,
};
#define MAKE_LMX2820_R19(r19_reserved_0, tempsense_en, r19_reserved_1) MAKE_LMX2820_REG_WR(R19, \
    (((r19_reserved_0) << R19_RESERVED_0_OFF) & R19_RESERVED_0_MSK) |  \
    (((tempsense_en) << TEMPSENSE_EN_OFF) & TEMPSENSE_EN_MSK) |  \
    (((r19_reserved_1) << R19_RESERVED_1_OFF) & R19_RESERVED_1_MSK))
// Register R20 [0x14] -- R20

enum r20_fields_t {
    R20_RESERVED_0_OFF = 0x9,
    R20_RESERVED_0_MSK = 0xfe00,
    VCO_DACISET_OFF = 0x0,
    VCO_DACISET_MSK = 0x1ff,
};
#define MAKE_LMX2820_R20(r20_reserved_0, vco_daciset) MAKE_LMX2820_REG_WR(R20, \
    (((r20_reserved_0) << R20_RESERVED_0_OFF) & R20_RESERVED_0_MSK) |  \
    (((vco_daciset) << VCO_DACISET_OFF) & VCO_DACISET_MSK))
// Register R21 [0x15] -- R21

enum r21_fields_t {
    R21_RESERVED_0_OFF = 0x0,
    R21_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R21(r21_reserved_0) MAKE_LMX2820_REG_WR(R21, \
    (((r21_reserved_0) << R21_RESERVED_0_OFF) & R21_RESERVED_0_MSK))
// Register R22 [0x16] -- R22
enum vco_sel_options {
    VCO_SEL_VCO1 = 1,
    VCO_SEL_VCO2 = 2,
    VCO_SEL_VCO3 = 3,
    VCO_SEL_VCO4 = 4,
    VCO_SEL_VCO5 = 5,
    VCO_SEL_VCO6 = 6,
    VCO_SEL_VCO7 = 7,
};

enum r22_fields_t {
    VCO_SEL_OFF = 0xd,
    VCO_SEL_MSK = 0xe000,
    R22_RESERVED_0_OFF = 0x8,
    R22_RESERVED_0_MSK = 0x1f00,
    VCO_CAPCTRL_OFF = 0x0,
    VCO_CAPCTRL_MSK = 0xff,
};
#define MAKE_LMX2820_R22(vco_sel, r22_reserved_0, vco_capctrl) MAKE_LMX2820_REG_WR(R22, \
    (((vco_sel) << VCO_SEL_OFF) & VCO_SEL_MSK) |  \
    (((r22_reserved_0) << R22_RESERVED_0_OFF) & R22_RESERVED_0_MSK) |  \
    (((vco_capctrl) << VCO_CAPCTRL_OFF) & VCO_CAPCTRL_MSK))
// Register R23 [0x17] -- R23
enum vco_sel_force_options {
    VCO_SEL_FORCE_DISABLED = 0,
    VCO_SEL_FORCE_ENABLED = 1,
};

enum r23_fields_t {
    R23_RESERVED_0_OFF = 0x1,
    R23_RESERVED_0_MSK = 0xfffe,
    VCO_SEL_FORCE_OFF = 0x0,
    VCO_SEL_FORCE_MSK = 0x1,
};
#define MAKE_LMX2820_R23(r23_reserved_0, vco_sel_force) MAKE_LMX2820_REG_WR(R23, \
    (((r23_reserved_0) << R23_RESERVED_0_OFF) & R23_RESERVED_0_MSK) |  \
    (((vco_sel_force) << VCO_SEL_FORCE_OFF) & VCO_SEL_FORCE_MSK))
// Register R24 [0x18] -- R24

enum r24_fields_t {
    R24_RESERVED_0_OFF = 0x0,
    R24_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R24(r24_reserved_0) MAKE_LMX2820_REG_WR(R24, \
    (((r24_reserved_0) << R24_RESERVED_0_OFF) & R24_RESERVED_0_MSK))
// Register R25 [0x19] -- R25

enum r25_fields_t {
    R25_RESERVED_0_OFF = 0x0,
    R25_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R25(r25_reserved_0) MAKE_LMX2820_REG_WR(R25, \
    (((r25_reserved_0) << R25_RESERVED_0_OFF) & R25_RESERVED_0_MSK))
// Register R26 [0x1a] -- R26

enum r26_fields_t {
    R26_RESERVED_0_OFF = 0x0,
    R26_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R26(r26_reserved_0) MAKE_LMX2820_REG_WR(R26, \
    (((r26_reserved_0) << R26_RESERVED_0_OFF) & R26_RESERVED_0_MSK))
// Register R27 [0x1b] -- R27

enum r27_fields_t {
    R27_RESERVED_0_OFF = 0x0,
    R27_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R27(r27_reserved_0) MAKE_LMX2820_REG_WR(R27, \
    (((r27_reserved_0) << R27_RESERVED_0_OFF) & R27_RESERVED_0_MSK))
// Register R28 [0x1c] -- R28

enum r28_fields_t {
    R28_RESERVED_0_OFF = 0x0,
    R28_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R28(r28_reserved_0) MAKE_LMX2820_REG_WR(R28, \
    (((r28_reserved_0) << R28_RESERVED_0_OFF) & R28_RESERVED_0_MSK))
// Register R29 [0x1d] -- R29

enum r29_fields_t {
    R29_RESERVED_0_OFF = 0x0,
    R29_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R29(r29_reserved_0) MAKE_LMX2820_REG_WR(R29, \
    (((r29_reserved_0) << R29_RESERVED_0_OFF) & R29_RESERVED_0_MSK))
// Register R30 [0x1e] -- R30

enum r30_fields_t {
    R30_RESERVED_0_OFF = 0x0,
    R30_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R30(r30_reserved_0) MAKE_LMX2820_REG_WR(R30, \
    (((r30_reserved_0) << R30_RESERVED_0_OFF) & R30_RESERVED_0_MSK))
// Register R31 [0x1f] -- R31

enum r31_fields_t {
    R31_RESERVED_0_OFF = 0x0,
    R31_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R31(r31_reserved_0) MAKE_LMX2820_REG_WR(R31, \
    (((r31_reserved_0) << R31_RESERVED_0_OFF) & R31_RESERVED_0_MSK))
// Register R32 [0x20] -- R32
enum chdivb_options {
    CHDIVB_0X0_EQ_DIVIDE_BY_2 = 0,
    CHDIVB_0X1_EQ_DIVIDE_BY_4 = 1,
    CHDIVB_0X2_EQ_DIVIDE_BY_8 = 2,
    CHDIVB_0X3_EQ_DIVIDE_BY_16 = 3,
    CHDIVB_0X4_EQ_DIVIDE_BY_32 = 4,
    CHDIVB_0X5_EQ_DIVIDE_BY_64 = 5,
    CHDIVB_0X6_EQ_DIVIDE_BY_128 = 6,
};
enum chdiva_options {
    CHDIVA_0X0_EQ_DIVIDE_BY_2 = 0,
    CHDIVA_0X1_EQ_DIVIDE_BY_4 = 1,
    CHDIVA_0X2_EQ_DIVIDE_BY_8 = 2,
    CHDIVA_0X3_EQ_DIVIDE_BY_16 = 3,
    CHDIVA_0X4_EQ_DIVIDE_BY_32 = 4,
    CHDIVA_0X5_EQ_DIVIDE_BY_64 = 5,
    CHDIVA_0X6_EQ_DIVIDE_BY_128 = 6,
};

enum r32_fields_t {
    R32_RESERVED_0_OFF = 0xc,
    R32_RESERVED_0_MSK = 0xf000,
    CHDIVB_OFF = 0x9,
    CHDIVB_MSK = 0xe00,
    CHDIVA_OFF = 0x6,
    CHDIVA_MSK = 0x1c0,
    R32_RESERVED_1_OFF = 0x0,
    R32_RESERVED_1_MSK = 0x3f,
};
#define MAKE_LMX2820_R32(r32_reserved_0, chdivb, chdiva, r32_reserved_1) MAKE_LMX2820_REG_WR(R32, \
    (((r32_reserved_0) << R32_RESERVED_0_OFF) & R32_RESERVED_0_MSK) |  \
    (((chdivb) << CHDIVB_OFF) & CHDIVB_MSK) |  \
    (((chdiva) << CHDIVA_OFF) & CHDIVA_MSK) |  \
    (((r32_reserved_1) << R32_RESERVED_1_OFF) & R32_RESERVED_1_MSK))
// Register R33 [0x21] -- R33

enum r33_fields_t {
    R33_RESERVED_0_OFF = 0x0,
    R33_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R33(r33_reserved_0) MAKE_LMX2820_REG_WR(R33, \
    (((r33_reserved_0) << R33_RESERVED_0_OFF) & R33_RESERVED_0_MSK))
// Register R34 [0x22] -- R34
enum loopback_en_options {
    LOOPBACK_EN_DISABLED = 0,
    LOOPBACK_EN_ENABLED = 1,
};
enum extvco_div_options {
    EXTVCO_DIV_DIVIDE_BY_2 = 0,
    EXTVCO_DIV_BYPASSED = 1,
};
enum extvco_en_options {
    EXTVCO_EN_DISABLED = 0,
    EXTVCO_EN_ENABLED = 1,
};

enum r34_fields_t {
    R34_RESERVED_0_OFF = 0xc,
    R34_RESERVED_0_MSK = 0xf000,
    LOOPBACK_EN_OFF = 0xb,
    LOOPBACK_EN_MSK = 0x800,
    R34_RESERVED_1_OFF = 0x5,
    R34_RESERVED_1_MSK = 0x7e0,
    EXTVCO_DIV_OFF = 0x4,
    EXTVCO_DIV_MSK = 0x10,
    R34_RESERVED_2_OFF = 0x1,
    R34_RESERVED_2_MSK = 0xe,
    EXTVCO_EN_OFF = 0x0,
    EXTVCO_EN_MSK = 0x1,
};
#define MAKE_LMX2820_R34(r34_reserved_0, loopback_en, r34_reserved_1, extvco_div, r34_reserved_2, extvco_en) MAKE_LMX2820_REG_WR(R34, \
    (((r34_reserved_0) << R34_RESERVED_0_OFF) & R34_RESERVED_0_MSK) |  \
    (((loopback_en) << LOOPBACK_EN_OFF) & LOOPBACK_EN_MSK) |  \
    (((r34_reserved_1) << R34_RESERVED_1_OFF) & R34_RESERVED_1_MSK) |  \
    (((extvco_div) << EXTVCO_DIV_OFF) & EXTVCO_DIV_MSK) |  \
    (((r34_reserved_2) << R34_RESERVED_2_OFF) & R34_RESERVED_2_MSK) |  \
    (((extvco_en) << EXTVCO_EN_OFF) & EXTVCO_EN_MSK))
// Register R35 [0x23] -- R35
enum mash_reset_n_options {
    MASH_RESET_N_RESET = 0,
    MASH_RESET_N_NORMAL_OPERATION = 1,
};
enum mash_order_options {
    MASH_ORDER_INTEGER_MODE = 0,
    MASH_ORDER_FIRST_ORDER = 1,
    MASH_ORDER_SECOND_ORDER = 2,
    MASH_ORDER_THIRD_ORDER = 3,
};
enum mashseed_en_options {
    MASHSEED_EN_DISABLED = 0,
    MASHSEED_EN_ENABLED = 1,
};

enum r35_fields_t {
    R35_RESERVED_0_OFF = 0xd,
    R35_RESERVED_0_MSK = 0xe000,
    MASH_RESET_N_OFF = 0xc,
    MASH_RESET_N_MSK = 0x1000,
    R35_RESERVED_1_OFF = 0x9,
    R35_RESERVED_1_MSK = 0xe00,
    MASH_ORDER_OFF = 0x7,
    MASH_ORDER_MSK = 0x180,
    MASHSEED_EN_OFF = 0x6,
    MASHSEED_EN_MSK = 0x40,
    R35_RESERVED_2_OFF = 0x0,
    R35_RESERVED_2_MSK = 0x3f,
};
#define MAKE_LMX2820_R35(r35_reserved_0, mash_reset_n, r35_reserved_1, mash_order, mashseed_en, r35_reserved_2) MAKE_LMX2820_REG_WR(R35, \
    (((r35_reserved_0) << R35_RESERVED_0_OFF) & R35_RESERVED_0_MSK) |  \
    (((mash_reset_n) << MASH_RESET_N_OFF) & MASH_RESET_N_MSK) |  \
    (((r35_reserved_1) << R35_RESERVED_1_OFF) & R35_RESERVED_1_MSK) |  \
    (((mash_order) << MASH_ORDER_OFF) & MASH_ORDER_MSK) |  \
    (((mashseed_en) << MASHSEED_EN_OFF) & MASHSEED_EN_MSK) |  \
    (((r35_reserved_2) << R35_RESERVED_2_OFF) & R35_RESERVED_2_MSK))
// Register R36 [0x24] -- R36

enum r36_fields_t {
    R36_RESERVED_0_OFF = 0xf,
    R36_RESERVED_0_MSK = 0x8000,
    PLL_N_OFF = 0x0,
    PLL_N_MSK = 0x7fff,
};
#define MAKE_LMX2820_R36(r36_reserved_0, pll_n) MAKE_LMX2820_REG_WR(R36, \
    (((r36_reserved_0) << R36_RESERVED_0_OFF) & R36_RESERVED_0_MSK) |  \
    (((pll_n) << PLL_N_OFF) & PLL_N_MSK))
// Register R37 [0x25] -- R37

enum r37_fields_t {
    R37_RESERVED_0_OFF = 0xf,
    R37_RESERVED_0_MSK = 0x8000,
    PFD_DLY_OFF = 0x9,
    PFD_DLY_MSK = 0x7e00,
    R37_RESERVED_1_OFF = 0x0,
    R37_RESERVED_1_MSK = 0x1ff,
};
#define MAKE_LMX2820_R37(r37_reserved_0, pfd_dly, r37_reserved_1) MAKE_LMX2820_REG_WR(R37, \
    (((r37_reserved_0) << R37_RESERVED_0_OFF) & R37_RESERVED_0_MSK) |  \
    (((pfd_dly) << PFD_DLY_OFF) & PFD_DLY_MSK) |  \
    (((r37_reserved_1) << R37_RESERVED_1_OFF) & R37_RESERVED_1_MSK))
// Register R38 [0x26] -- R38

enum r38_fields_t {
    PLL_DEN_U_OFF = 0x0,
    PLL_DEN_U_MSK = 0xffff,
};
#define MAKE_LMX2820_R38(pll_den_u) MAKE_LMX2820_REG_WR(R38, \
    (((pll_den_u) << PLL_DEN_U_OFF) & PLL_DEN_U_MSK))
// Register R39 [0x27] -- R39

enum r39_fields_t {
    PLL_DEN_L_OFF = 0x0,
    PLL_DEN_L_MSK = 0xffff,
};
#define MAKE_LMX2820_R39(pll_den_l) MAKE_LMX2820_REG_WR(R39, \
    (((pll_den_l) << PLL_DEN_L_OFF) & PLL_DEN_L_MSK))
// Register R40 [0x28] -- R40

enum r40_fields_t {
    MASH_SEED_U_OFF = 0x0,
    MASH_SEED_U_MSK = 0xffff,
};
#define MAKE_LMX2820_R40(mash_seed_u) MAKE_LMX2820_REG_WR(R40, \
    (((mash_seed_u) << MASH_SEED_U_OFF) & MASH_SEED_U_MSK))
// Register R41 [0x29] -- R41

enum r41_fields_t {
    MASH_SEED_L_OFF = 0x0,
    MASH_SEED_L_MSK = 0xffff,
};
#define MAKE_LMX2820_R41(mash_seed_l) MAKE_LMX2820_REG_WR(R41, \
    (((mash_seed_l) << MASH_SEED_L_OFF) & MASH_SEED_L_MSK))
// Register R42 [0x2a] -- R42

enum r42_fields_t {
    PLL_NUM_U_OFF = 0x0,
    PLL_NUM_U_MSK = 0xffff,
};
#define MAKE_LMX2820_R42(pll_num_u) MAKE_LMX2820_REG_WR(R42, \
    (((pll_num_u) << PLL_NUM_U_OFF) & PLL_NUM_U_MSK))
// Register R43 [0x2b] -- R43

enum r43_fields_t {
    PLL_NUM_L_OFF = 0x0,
    PLL_NUM_L_MSK = 0xffff,
};
#define MAKE_LMX2820_R43(pll_num_l) MAKE_LMX2820_REG_WR(R43, \
    (((pll_num_l) << PLL_NUM_L_OFF) & PLL_NUM_L_MSK))
// Register R44 [0x2c] -- R44

enum r44_fields_t {
    INSTCAL_PLL_NUM_U_OFF = 0x0,
    INSTCAL_PLL_NUM_U_MSK = 0xffff,
};
#define MAKE_LMX2820_R44(instcal_pll_num_u) MAKE_LMX2820_REG_WR(R44, \
    (((instcal_pll_num_u) << INSTCAL_PLL_NUM_U_OFF) & INSTCAL_PLL_NUM_U_MSK))
// Register R45 [0x2d] -- R45

enum r45_fields_t {
    INSTCAL_PLL_NUM_L_OFF = 0x0,
    INSTCAL_PLL_NUM_L_MSK = 0xffff,
};
#define MAKE_LMX2820_R45(instcal_pll_num_l) MAKE_LMX2820_REG_WR(R45, \
    (((instcal_pll_num_l) << INSTCAL_PLL_NUM_L_OFF) & INSTCAL_PLL_NUM_L_MSK))
// Register R46 [0x2e] -- R46

enum r46_fields_t {
    R46_RESERVED_0_OFF = 0x0,
    R46_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R46(r46_reserved_0) MAKE_LMX2820_REG_WR(R46, \
    (((r46_reserved_0) << R46_RESERVED_0_OFF) & R46_RESERVED_0_MSK))
// Register R47 [0x2f] -- R47

enum r47_fields_t {
    R47_RESERVED_0_OFF = 0x0,
    R47_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R47(r47_reserved_0) MAKE_LMX2820_REG_WR(R47, \
    (((r47_reserved_0) << R47_RESERVED_0_OFF) & R47_RESERVED_0_MSK))
// Register R48 [0x30] -- R48

enum r48_fields_t {
    R48_RESERVED_0_OFF = 0x0,
    R48_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R48(r48_reserved_0) MAKE_LMX2820_REG_WR(R48, \
    (((r48_reserved_0) << R48_RESERVED_0_OFF) & R48_RESERVED_0_MSK))
// Register R49 [0x31] -- R49

enum r49_fields_t {
    R49_RESERVED_0_OFF = 0x0,
    R49_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R49(r49_reserved_0) MAKE_LMX2820_REG_WR(R49, \
    (((r49_reserved_0) << R49_RESERVED_0_OFF) & R49_RESERVED_0_MSK))
// Register R50 [0x32] -- R50

enum r50_fields_t {
    R50_RESERVED_0_OFF = 0x0,
    R50_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R50(r50_reserved_0) MAKE_LMX2820_REG_WR(R50, \
    (((r50_reserved_0) << R50_RESERVED_0_OFF) & R50_RESERVED_0_MSK))
// Register R51 [0x33] -- R51

enum r51_fields_t {
    R51_RESERVED_0_OFF = 0x0,
    R51_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R51(r51_reserved_0) MAKE_LMX2820_REG_WR(R51, \
    (((r51_reserved_0) << R51_RESERVED_0_OFF) & R51_RESERVED_0_MSK))
// Register R52 [0x34] -- R52

enum r52_fields_t {
    R52_RESERVED_0_OFF = 0x0,
    R52_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R52(r52_reserved_0) MAKE_LMX2820_REG_WR(R52, \
    (((r52_reserved_0) << R52_RESERVED_0_OFF) & R52_RESERVED_0_MSK))
// Register R53 [0x35] -- R53

enum r53_fields_t {
    R53_RESERVED_0_OFF = 0x0,
    R53_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R53(r53_reserved_0) MAKE_LMX2820_REG_WR(R53, \
    (((r53_reserved_0) << R53_RESERVED_0_OFF) & R53_RESERVED_0_MSK))
// Register R54 [0x36] -- R54

enum r54_fields_t {
    R54_RESERVED_0_OFF = 0x0,
    R54_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R54(r54_reserved_0) MAKE_LMX2820_REG_WR(R54, \
    (((r54_reserved_0) << R54_RESERVED_0_OFF) & R54_RESERVED_0_MSK))
// Register R55 [0x37] -- R55

enum r55_fields_t {
    R55_RESERVED_0_OFF = 0x0,
    R55_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R55(r55_reserved_0) MAKE_LMX2820_REG_WR(R55, \
    (((r55_reserved_0) << R55_RESERVED_0_OFF) & R55_RESERVED_0_MSK))
// Register R56 [0x38] -- R56

enum r56_fields_t {
    R56_RESERVED_0_OFF = 0x6,
    R56_RESERVED_0_MSK = 0xffc0,
    EXTPFD_DIV_OFF = 0x0,
    EXTPFD_DIV_MSK = 0x3f,
};
#define MAKE_LMX2820_R56(r56_reserved_0, extpfd_div) MAKE_LMX2820_REG_WR(R56, \
    (((r56_reserved_0) << R56_RESERVED_0_OFF) & R56_RESERVED_0_MSK) |  \
    (((extpfd_div) << EXTPFD_DIV_OFF) & EXTPFD_DIV_MSK))
// Register R57 [0x39] -- R57
enum pfd_sel_options {
    PFD_SEL_ENABLED = 0,
    PFD_SEL_DISABLED = 1,
};

enum r57_fields_t {
    R57_RESERVED_0_OFF = 0x1,
    R57_RESERVED_0_MSK = 0xfffe,
    PFD_SEL_OFF = 0x0,
    PFD_SEL_MSK = 0x1,
};
#define MAKE_LMX2820_R57(r57_reserved_0, pfd_sel) MAKE_LMX2820_REG_WR(R57, \
    (((r57_reserved_0) << R57_RESERVED_0_OFF) & R57_RESERVED_0_MSK) |  \
    (((pfd_sel) << PFD_SEL_OFF) & PFD_SEL_MSK))
// Register R58 [0x3a] -- R58

enum r58_fields_t {
    R58_RESERVED_0_OFF = 0x0,
    R58_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R58(r58_reserved_0) MAKE_LMX2820_REG_WR(R58, \
    (((r58_reserved_0) << R58_RESERVED_0_OFF) & R58_RESERVED_0_MSK))
// Register R59 [0x3b] -- R59

enum r59_fields_t {
    R59_RESERVED_0_OFF = 0x0,
    R59_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R59(r59_reserved_0) MAKE_LMX2820_REG_WR(R59, \
    (((r59_reserved_0) << R59_RESERVED_0_OFF) & R59_RESERVED_0_MSK))
// Register R60 [0x3c] -- R60

enum r60_fields_t {
    R60_RESERVED_0_OFF = 0x0,
    R60_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R60(r60_reserved_0) MAKE_LMX2820_REG_WR(R60, \
    (((r60_reserved_0) << R60_RESERVED_0_OFF) & R60_RESERVED_0_MSK))
// Register R61 [0x3d] -- R61

enum r61_fields_t {
    R61_RESERVED_0_OFF = 0x0,
    R61_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R61(r61_reserved_0) MAKE_LMX2820_REG_WR(R61, \
    (((r61_reserved_0) << R61_RESERVED_0_OFF) & R61_RESERVED_0_MSK))
// Register R62 [0x3e] -- R62

enum r62_fields_t {
    MASH_RST_COUNT_U_OFF = 0x0,
    MASH_RST_COUNT_U_MSK = 0xffff,
};
#define MAKE_LMX2820_R62(mash_rst_count_u) MAKE_LMX2820_REG_WR(R62, \
    (((mash_rst_count_u) << MASH_RST_COUNT_U_OFF) & MASH_RST_COUNT_U_MSK))
// Register R63 [0x3f] -- R63

enum r63_fields_t {
    MASH_RST_COUNT_L_OFF = 0x0,
    MASH_RST_COUNT_L_MSK = 0xffff,
};
#define MAKE_LMX2820_R63(mash_rst_count_l) MAKE_LMX2820_REG_WR(R63, \
    (((mash_rst_count_l) << MASH_RST_COUNT_L_OFF) & MASH_RST_COUNT_L_MSK))
// Register R64 [0x40] -- R64
enum sysref_inp_fmt_options {
    SYSREF_INP_FMT_CMOS_INPUT_AT_SRREQ_P_PIN__1_8_V_TO_3_3_V_LOGIC = 0,
    SYSREF_INP_FMT_AC_COUPLE_CMOS_INPUT_AT_SRREQ_P_PIN = 1,
    SYSREF_INP_FMT_AC_COUPLED_DIFFERENTIAL_LVDS_INPUT__REQUIRES_EXTERNAL_100_OHM_DIFFERENTIAL_TERMINATION = 2,
    SYSREF_INP_FMT_DC_COUPLED_DIFFERENTIAL_LVDS_INPUT__REQUIRES_EXTERNAL_100_OHM_DIFFERENTIAL_TERMINATION = 3,
};
enum sysref_div_pre_options {
    SYSREF_DIV_PRE_DIVIDE_BY_2 = 1,
    SYSREF_DIV_PRE_DIVIDE_BY_4 = 2,
    SYSREF_DIV_PRE_DIVIDE_BY_8 = 4,
};
enum sysref_repeat_ns_options {
    SYSREF_REPEAT_NS_IF_SYSREF_REPEAT_EQ_1 = 0,
    SYSREF_REPEAT_NS_ENABLED = 1,
};
enum sysref_pulse_options {
    SYSREF_PULSE_CONTINUOUS_MODE = 0,
    SYSREF_PULSE_PULSED_MODE = 1,
};
enum sysref_en_options {
    SYSREF_EN_DISABLED = 0,
    SYSREF_EN_ENABLED = 1,
};
enum sysref_repeat_options {
    SYSREF_REPEAT_MASTER_MODE = 0,
    SYSREF_REPEAT_REPEATER_MODE = 1,
};

enum r64_fields_t {
    R64_RESERVED_0_OFF = 0xa,
    R64_RESERVED_0_MSK = 0xfc00,
    SYSREF_INP_FMT_OFF = 0x8,
    SYSREF_INP_FMT_MSK = 0x300,
    SYSREF_DIV_PRE_OFF = 0x5,
    SYSREF_DIV_PRE_MSK = 0xe0,
    SYSREF_REPEAT_NS_OFF = 0x4,
    SYSREF_REPEAT_NS_MSK = 0x10,
    SYSREF_PULSE_OFF = 0x3,
    SYSREF_PULSE_MSK = 0x8,
    SYSREF_EN_OFF = 0x2,
    SYSREF_EN_MSK = 0x4,
    SYSREF_REPEAT_OFF = 0x1,
    SYSREF_REPEAT_MSK = 0x2,
    R64_RESERVED_1_OFF = 0x0,
    R64_RESERVED_1_MSK = 0x1,
};
#define MAKE_LMX2820_R64(r64_reserved_0, sysref_inp_fmt, sysref_div_pre, sysref_repeat_ns, sysref_pulse, sysref_en, sysref_repeat, r64_reserved_1) MAKE_LMX2820_REG_WR(R64, \
    (((r64_reserved_0) << R64_RESERVED_0_OFF) & R64_RESERVED_0_MSK) |  \
    (((sysref_inp_fmt) << SYSREF_INP_FMT_OFF) & SYSREF_INP_FMT_MSK) |  \
    (((sysref_div_pre) << SYSREF_DIV_PRE_OFF) & SYSREF_DIV_PRE_MSK) |  \
    (((sysref_repeat_ns) << SYSREF_REPEAT_NS_OFF) & SYSREF_REPEAT_NS_MSK) |  \
    (((sysref_pulse) << SYSREF_PULSE_OFF) & SYSREF_PULSE_MSK) |  \
    (((sysref_en) << SYSREF_EN_OFF) & SYSREF_EN_MSK) |  \
    (((sysref_repeat) << SYSREF_REPEAT_OFF) & SYSREF_REPEAT_MSK) |  \
    (((r64_reserved_1) << R64_RESERVED_1_OFF) & R64_RESERVED_1_MSK))
// Register R65 [0x41] -- R65

enum r65_fields_t {
    R65_RESERVED_0_OFF = 0xb,
    R65_RESERVED_0_MSK = 0xf800,
    SYSREF_DIV_OFF = 0x0,
    SYSREF_DIV_MSK = 0x7ff,
};
#define MAKE_LMX2820_R65(r65_reserved_0, sysref_div) MAKE_LMX2820_REG_WR(R65, \
    (((r65_reserved_0) << R65_RESERVED_0_OFF) & R65_RESERVED_0_MSK) |  \
    (((sysref_div) << SYSREF_DIV_OFF) & SYSREF_DIV_MSK))
// Register R66 [0x42] -- R66

enum r66_fields_t {
    R66_RESERVED_0_OFF = 0xc,
    R66_RESERVED_0_MSK = 0xf000,
    JESD_DAC2_CTRL_OFF = 0x6,
    JESD_DAC2_CTRL_MSK = 0xfc0,
    JESD_DAC1_CTRL_OFF = 0x0,
    JESD_DAC1_CTRL_MSK = 0x3f,
};
#define MAKE_LMX2820_R66(r66_reserved_0, jesd_dac2_ctrl, jesd_dac1_ctrl) MAKE_LMX2820_REG_WR(R66, \
    (((r66_reserved_0) << R66_RESERVED_0_OFF) & R66_RESERVED_0_MSK) |  \
    (((jesd_dac2_ctrl) << JESD_DAC2_CTRL_OFF) & JESD_DAC2_CTRL_MSK) |  \
    (((jesd_dac1_ctrl) << JESD_DAC1_CTRL_OFF) & JESD_DAC1_CTRL_MSK))
// Register R67 [0x43] -- R67

enum r67_fields_t {
    SYSREF_PULSE_CNT_OFF = 0xc,
    SYSREF_PULSE_CNT_MSK = 0xf000,
    JESD_DAC4_CTRL_OFF = 0x6,
    JESD_DAC4_CTRL_MSK = 0xfc0,
    JESD_DAC3_CTRL_OFF = 0x0,
    JESD_DAC3_CTRL_MSK = 0x3f,
};
#define MAKE_LMX2820_R67(sysref_pulse_cnt, jesd_dac4_ctrl, jesd_dac3_ctrl) MAKE_LMX2820_REG_WR(R67, \
    (((sysref_pulse_cnt) << SYSREF_PULSE_CNT_OFF) & SYSREF_PULSE_CNT_MSK) |  \
    (((jesd_dac4_ctrl) << JESD_DAC4_CTRL_OFF) & JESD_DAC4_CTRL_MSK) |  \
    (((jesd_dac3_ctrl) << JESD_DAC3_CTRL_OFF) & JESD_DAC3_CTRL_MSK))
// Register R68 [0x44] -- R68
enum inpin_ignore_options {
    INPIN_IGNORE_ENABLES = 0,
    INPIN_IGNORE_DISABLES_PIN = 1,
};
enum psync_inp_fmt_options {
    PSYNC_INP_FMT_CMOS_INPUT__1_8_V_TO_3_3_V_LOGIC = 0,
    PSYNC_INP_FMT_AC_COUPLED_DIFFERENTIAL_LVDS_INPUT__REQUIRES_EXTERNAL_100_OHM_DIFFERENTIAL_TERMINATION = 1,
};

enum r68_fields_t {
    R68_RESERVED_0_OFF = 0x6,
    R68_RESERVED_0_MSK = 0xffc0,
    INPIN_IGNORE_OFF = 0x5,
    INPIN_IGNORE_MSK = 0x20,
    R68_RESERVED_1_OFF = 0x1,
    R68_RESERVED_1_MSK = 0x1e,
    PSYNC_INP_FMT_OFF = 0x0,
    PSYNC_INP_FMT_MSK = 0x1,
};
#define MAKE_LMX2820_R68(r68_reserved_0, inpin_ignore, r68_reserved_1, psync_inp_fmt) MAKE_LMX2820_REG_WR(R68, \
    (((r68_reserved_0) << R68_RESERVED_0_OFF) & R68_RESERVED_0_MSK) |  \
    (((inpin_ignore) << INPIN_IGNORE_OFF) & INPIN_IGNORE_MSK) |  \
    (((r68_reserved_1) << R68_RESERVED_1_OFF) & R68_RESERVED_1_MSK) |  \
    (((psync_inp_fmt) << PSYNC_INP_FMT_OFF) & PSYNC_INP_FMT_MSK))
// Register R69 [0x45] -- R69
enum srout_pd_options {
    SROUT_PD_NORMAL_OPERATION = 0,
    SROUT_PD_POWER_DOWN = 1,
};

enum r69_fields_t {
    R69_RESERVED_0_OFF = 0x5,
    R69_RESERVED_0_MSK = 0xffe0,
    SROUT_PD_OFF = 0x4,
    SROUT_PD_MSK = 0x10,
    R69_RESERVED_1_OFF = 0x0,
    R69_RESERVED_1_MSK = 0xf,
};
#define MAKE_LMX2820_R69(r69_reserved_0, srout_pd, r69_reserved_1) MAKE_LMX2820_REG_WR(R69, \
    (((r69_reserved_0) << R69_RESERVED_0_OFF) & R69_RESERVED_0_MSK) |  \
    (((srout_pd) << SROUT_PD_OFF) & SROUT_PD_MSK) |  \
    (((r69_reserved_1) << R69_RESERVED_1_OFF) & R69_RESERVED_1_MSK))
// Register R70 [0x46] -- R70
enum dblbuf_outmux_en_options {
    DBLBUF_OUTMUX_EN_DISABLED = 0,
    DBLBUF_OUTMUX_EN_ENABLED = 1,
};
enum dblbuf_outbuf_en_options {
    DBLBUF_OUTBUF_EN_DISABLED = 0,
    DBLBUF_OUTBUF_EN_ENABLED = 1,
};
enum dblbuf_chdiv_en_options {
    DBLBUF_CHDIV_EN_DISABLED = 0,
    DBLBUF_CHDIV_EN_ENABLED = 1,
};
enum dblbuf_pll_en_options {
    DBLBUF_PLL_EN_DISABLED = 0,
    DBLBUF_PLL_EN_ENABLED = 1,
};

enum r70_fields_t {
    R70_RESERVED_0_OFF = 0x8,
    R70_RESERVED_0_MSK = 0xff00,
    DBLBUF_OUTMUX_EN_OFF = 0x7,
    DBLBUF_OUTMUX_EN_MSK = 0x80,
    DBLBUF_OUTBUF_EN_OFF = 0x6,
    DBLBUF_OUTBUF_EN_MSK = 0x40,
    DBLBUF_CHDIV_EN_OFF = 0x5,
    DBLBUF_CHDIV_EN_MSK = 0x20,
    DBLBUF_PLL_EN_OFF = 0x4,
    DBLBUF_PLL_EN_MSK = 0x10,
    R70_RESERVED_1_OFF = 0x0,
    R70_RESERVED_1_MSK = 0xf,
};
#define MAKE_LMX2820_R70(r70_reserved_0, dblbuf_outmux_en, dblbuf_outbuf_en, dblbuf_chdiv_en, dblbuf_pll_en, r70_reserved_1) MAKE_LMX2820_REG_WR(R70, \
    (((r70_reserved_0) << R70_RESERVED_0_OFF) & R70_RESERVED_0_MSK) |  \
    (((dblbuf_outmux_en) << DBLBUF_OUTMUX_EN_OFF) & DBLBUF_OUTMUX_EN_MSK) |  \
    (((dblbuf_outbuf_en) << DBLBUF_OUTBUF_EN_OFF) & DBLBUF_OUTBUF_EN_MSK) |  \
    (((dblbuf_chdiv_en) << DBLBUF_CHDIV_EN_OFF) & DBLBUF_CHDIV_EN_MSK) |  \
    (((dblbuf_pll_en) << DBLBUF_PLL_EN_OFF) & DBLBUF_PLL_EN_MSK) |  \
    (((r70_reserved_1) << R70_RESERVED_1_OFF) & R70_RESERVED_1_MSK))
// Register R71 [0x47] -- R71

enum r71_fields_t {
    R71_RESERVED_0_OFF = 0x0,
    R71_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R71(r71_reserved_0) MAKE_LMX2820_REG_WR(R71, \
    (((r71_reserved_0) << R71_RESERVED_0_OFF) & R71_RESERVED_0_MSK))
// Register R72 [0x48] -- R72

enum r72_fields_t {
    R72_RESERVED_0_OFF = 0x0,
    R72_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R72(r72_reserved_0) MAKE_LMX2820_REG_WR(R72, \
    (((r72_reserved_0) << R72_RESERVED_0_OFF) & R72_RESERVED_0_MSK))
// Register R73 [0x49] -- R73

enum r73_fields_t {
    R73_RESERVED_0_OFF = 0x0,
    R73_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R73(r73_reserved_0) MAKE_LMX2820_REG_WR(R73, \
    (((r73_reserved_0) << R73_RESERVED_0_OFF) & R73_RESERVED_0_MSK))
// Register R74 [0x4a] -- R74
enum rb_ld_options {
    RB_LD_UNLOCKED0 = 0,
    RB_LD_UNLOCKED1 = 1,
    RB_LD_LOCKED = 2,
    RB_LD_INVALID = 3,
};
enum rb_vco_sel_options {
    RB_VCO_SEL_INVALID = 0,
    RB_VCO_SEL_VCO1 = 1,
    RB_VCO_SEL_VCO2 = 2,
    RB_VCO_SEL_VCO3 = 3,
    RB_VCO_SEL_VCO4 = 4,
    RB_VCO_SEL_VCO5 = 5,
    RB_VCO_SEL_VCO6 = 6,
    RB_VCO_SEL_VCO7 = 7,
};

enum r74_fields_t {
    RB_LD_OFF = 0xe,
    RB_LD_MSK = 0xc000,
    R74_RESERVED_0_OFF = 0xd,
    R74_RESERVED_0_MSK = 0x2000,
    RB_VCO_CAPCTRL_OFF = 0x5,
    RB_VCO_CAPCTRL_MSK = 0x1fe0,
    RB_VCO_SEL_OFF = 0x2,
    RB_VCO_SEL_MSK = 0x1c,
    R74_RESERVED_1_OFF = 0x0,
    R74_RESERVED_1_MSK = 0x3,
};
#define MAKE_LMX2820_R74(rb_ld, r74_reserved_0, rb_vco_capctrl, rb_vco_sel, r74_reserved_1) MAKE_LMX2820_REG_WR(R74, \
    (((rb_ld) << RB_LD_OFF) & RB_LD_MSK) |  \
    (((r74_reserved_0) << R74_RESERVED_0_OFF) & R74_RESERVED_0_MSK) |  \
    (((rb_vco_capctrl) << RB_VCO_CAPCTRL_OFF) & RB_VCO_CAPCTRL_MSK) |  \
    (((rb_vco_sel) << RB_VCO_SEL_OFF) & RB_VCO_SEL_MSK) |  \
    (((r74_reserved_1) << R74_RESERVED_1_OFF) & R74_RESERVED_1_MSK))
// Register R75 [0x4b] -- R75

enum r75_fields_t {
    R75_RESERVED_0_OFF = 0x9,
    R75_RESERVED_0_MSK = 0xfe00,
    RB_VCO_DACISET_OFF = 0x0,
    RB_VCO_DACISET_MSK = 0x1ff,
};
#define MAKE_LMX2820_R75(r75_reserved_0, rb_vco_daciset) MAKE_LMX2820_REG_WR(R75, \
    (((r75_reserved_0) << R75_RESERVED_0_OFF) & R75_RESERVED_0_MSK) |  \
    (((rb_vco_daciset) << RB_VCO_DACISET_OFF) & RB_VCO_DACISET_MSK))
// Register R76 [0x4c] -- R76

enum r76_fields_t {
    R76_RESERVED_0_OFF = 0xb,
    R76_RESERVED_0_MSK = 0xf800,
    RB_TEMP_SENS_OFF = 0x0,
    RB_TEMP_SENS_MSK = 0x7ff,
};
#define MAKE_LMX2820_R76(r76_reserved_0, rb_temp_sens) MAKE_LMX2820_REG_WR(R76, \
    (((r76_reserved_0) << R76_RESERVED_0_OFF) & R76_RESERVED_0_MSK) |  \
    (((rb_temp_sens) << RB_TEMP_SENS_OFF) & RB_TEMP_SENS_MSK))
// Register R77 [0x4d] -- R77
enum pinmute_pol_options {
    PINMUTE_POL_ACTIVE_HIGH = 0,
    PINMUTE_POL_ACTIVE_LOW = 1,
};

enum r77_fields_t {
    R77_RESERVED_0_OFF = 0x9,
    R77_RESERVED_0_MSK = 0xfe00,
    PINMUTE_POL_OFF = 0x8,
    PINMUTE_POL_MSK = 0x100,
    R77_RESERVED_1_OFF = 0x0,
    R77_RESERVED_1_MSK = 0xff,
};
#define MAKE_LMX2820_R77(r77_reserved_0, pinmute_pol, r77_reserved_1) MAKE_LMX2820_REG_WR(R77, \
    (((r77_reserved_0) << R77_RESERVED_0_OFF) & R77_RESERVED_0_MSK) |  \
    (((pinmute_pol) << PINMUTE_POL_OFF) & PINMUTE_POL_MSK) |  \
    (((r77_reserved_1) << R77_RESERVED_1_OFF) & R77_RESERVED_1_MSK))
// Register R78 [0x4e] -- R78
enum outa_pd_options {
    OUTA_PD_NORMAL_OPERATION = 0,
    OUTA_PD_POWER_DOWN = 1,
};
enum outa_mux_options {
    OUTA_MUX_CHANNEL_DIVIDER = 0,
    OUTA_MUX_VCO = 1,
    OUTA_MUX_VCO_DOUBLER = 2,
};

enum r78_fields_t {
    R78_RESERVED_0_OFF = 0x5,
    R78_RESERVED_0_MSK = 0xffe0,
    OUTA_PD_OFF = 0x4,
    OUTA_PD_MSK = 0x10,
    R78_RESERVED_1_OFF = 0x2,
    R78_RESERVED_1_MSK = 0xc,
    OUTA_MUX_OFF = 0x0,
    OUTA_MUX_MSK = 0x3,
};
#define MAKE_LMX2820_R78(r78_reserved_0, outa_pd, r78_reserved_1, outa_mux) MAKE_LMX2820_REG_WR(R78, \
    (((r78_reserved_0) << R78_RESERVED_0_OFF) & R78_RESERVED_0_MSK) |  \
    (((outa_pd) << OUTA_PD_OFF) & OUTA_PD_MSK) |  \
    (((r78_reserved_1) << R78_RESERVED_1_OFF) & R78_RESERVED_1_MSK) |  \
    (((outa_mux) << OUTA_MUX_OFF) & OUTA_MUX_MSK))
// Register R79 [0x4f] -- R79
enum outb_pd_options {
    OUTB_PD_NORMAL_OPERATION = 0,
    OUTB_PD_POWER_DOWN = 1,
};
enum outb_mux_options {
    OUTB_MUX_CHANNEL_DIVIDER = 0,
    OUTB_MUX_VCO = 1,
    OUTB_MUX_VCO_DOUBLER = 2,
};

enum r79_fields_t {
    R79_RESERVED_0_OFF = 0x9,
    R79_RESERVED_0_MSK = 0xfe00,
    OUTB_PD_OFF = 0x8,
    OUTB_PD_MSK = 0x100,
    R79_RESERVED_1_OFF = 0x6,
    R79_RESERVED_1_MSK = 0xc0,
    OUTB_MUX_OFF = 0x4,
    OUTB_MUX_MSK = 0x30,
    OUTA_PWR_OFF = 0x1,
    OUTA_PWR_MSK = 0xe,
    R79_RESERVED_2_OFF = 0x0,
    R79_RESERVED_2_MSK = 0x1,
};
#define MAKE_LMX2820_R79(r79_reserved_0, outb_pd, r79_reserved_1, outb_mux, outa_pwr, r79_reserved_2) MAKE_LMX2820_REG_WR(R79, \
    (((r79_reserved_0) << R79_RESERVED_0_OFF) & R79_RESERVED_0_MSK) |  \
    (((outb_pd) << OUTB_PD_OFF) & OUTB_PD_MSK) |  \
    (((r79_reserved_1) << R79_RESERVED_1_OFF) & R79_RESERVED_1_MSK) |  \
    (((outb_mux) << OUTB_MUX_OFF) & OUTB_MUX_MSK) |  \
    (((outa_pwr) << OUTA_PWR_OFF) & OUTA_PWR_MSK) |  \
    (((r79_reserved_2) << R79_RESERVED_2_OFF) & R79_RESERVED_2_MSK))
// Register R80 [0x50] -- R80

enum r80_fields_t {
    R80_RESERVED_0_OFF = 0x9,
    R80_RESERVED_0_MSK = 0xfe00,
    OUTB_PWR_OFF = 0x6,
    OUTB_PWR_MSK = 0x1c0,
    R80_RESERVED_1_OFF = 0x0,
    R80_RESERVED_1_MSK = 0x3f,
};
#define MAKE_LMX2820_R80(r80_reserved_0, outb_pwr, r80_reserved_1) MAKE_LMX2820_REG_WR(R80, \
    (((r80_reserved_0) << R80_RESERVED_0_OFF) & R80_RESERVED_0_MSK) |  \
    (((outb_pwr) << OUTB_PWR_OFF) & OUTB_PWR_MSK) |  \
    (((r80_reserved_1) << R80_RESERVED_1_OFF) & R80_RESERVED_1_MSK))
// Register R81 [0x51] -- R81

enum r81_fields_t {
    R81_RESERVED_0_OFF = 0x0,
    R81_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R81(r81_reserved_0) MAKE_LMX2820_REG_WR(R81, \
    (((r81_reserved_0) << R81_RESERVED_0_OFF) & R81_RESERVED_0_MSK))
// Register R82 [0x52] -- R82

enum r82_fields_t {
    R82_RESERVED_0_OFF = 0x0,
    R82_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R82(r82_reserved_0) MAKE_LMX2820_REG_WR(R82, \
    (((r82_reserved_0) << R82_RESERVED_0_OFF) & R82_RESERVED_0_MSK))
// Register R83 [0x53] -- R83

enum r83_fields_t {
    R83_RESERVED_0_OFF = 0x0,
    R83_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R83(r83_reserved_0) MAKE_LMX2820_REG_WR(R83, \
    (((r83_reserved_0) << R83_RESERVED_0_OFF) & R83_RESERVED_0_MSK))
// Register R84 [0x54] -- R84

enum r84_fields_t {
    R84_RESERVED_0_OFF = 0x0,
    R84_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R84(r84_reserved_0) MAKE_LMX2820_REG_WR(R84, \
    (((r84_reserved_0) << R84_RESERVED_0_OFF) & R84_RESERVED_0_MSK))
// Register R85 [0x55] -- R85

enum r85_fields_t {
    R85_RESERVED_0_OFF = 0x0,
    R85_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R85(r85_reserved_0) MAKE_LMX2820_REG_WR(R85, \
    (((r85_reserved_0) << R85_RESERVED_0_OFF) & R85_RESERVED_0_MSK))
// Register R86 [0x56] -- R86

enum r86_fields_t {
    R86_RESERVED_0_OFF = 0x0,
    R86_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R86(r86_reserved_0) MAKE_LMX2820_REG_WR(R86, \
    (((r86_reserved_0) << R86_RESERVED_0_OFF) & R86_RESERVED_0_MSK))
// Register R87 [0x57] -- R87

enum r87_fields_t {
    R87_RESERVED_0_OFF = 0x0,
    R87_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R87(r87_reserved_0) MAKE_LMX2820_REG_WR(R87, \
    (((r87_reserved_0) << R87_RESERVED_0_OFF) & R87_RESERVED_0_MSK))
// Register R88 [0x58] -- R88

enum r88_fields_t {
    R88_RESERVED_0_OFF = 0x0,
    R88_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R88(r88_reserved_0) MAKE_LMX2820_REG_WR(R88, \
    (((r88_reserved_0) << R88_RESERVED_0_OFF) & R88_RESERVED_0_MSK))
// Register R89 [0x59] -- R89

enum r89_fields_t {
    R89_RESERVED_0_OFF = 0x0,
    R89_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R89(r89_reserved_0) MAKE_LMX2820_REG_WR(R89, \
    (((r89_reserved_0) << R89_RESERVED_0_OFF) & R89_RESERVED_0_MSK))
// Register R90 [0x5a] -- R90

enum r90_fields_t {
    R90_RESERVED_0_OFF = 0x0,
    R90_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R90(r90_reserved_0) MAKE_LMX2820_REG_WR(R90, \
    (((r90_reserved_0) << R90_RESERVED_0_OFF) & R90_RESERVED_0_MSK))
// Register R91 [0x5b] -- R91

enum r91_fields_t {
    R91_RESERVED_0_OFF = 0x0,
    R91_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R91(r91_reserved_0) MAKE_LMX2820_REG_WR(R91, \
    (((r91_reserved_0) << R91_RESERVED_0_OFF) & R91_RESERVED_0_MSK))
// Register R92 [0x5c] -- R92

enum r92_fields_t {
    R92_RESERVED_0_OFF = 0x0,
    R92_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R92(r92_reserved_0) MAKE_LMX2820_REG_WR(R92, \
    (((r92_reserved_0) << R92_RESERVED_0_OFF) & R92_RESERVED_0_MSK))
// Register R93 [0x5d] -- R93

enum r93_fields_t {
    R93_RESERVED_0_OFF = 0x0,
    R93_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R93(r93_reserved_0) MAKE_LMX2820_REG_WR(R93, \
    (((r93_reserved_0) << R93_RESERVED_0_OFF) & R93_RESERVED_0_MSK))
// Register R94 [0x5e] -- R94

enum r94_fields_t {
    R94_RESERVED_0_OFF = 0x0,
    R94_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R94(r94_reserved_0) MAKE_LMX2820_REG_WR(R94, \
    (((r94_reserved_0) << R94_RESERVED_0_OFF) & R94_RESERVED_0_MSK))
// Register R95 [0x5f] -- R95

enum r95_fields_t {
    R95_RESERVED_0_OFF = 0x0,
    R95_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R95(r95_reserved_0) MAKE_LMX2820_REG_WR(R95, \
    (((r95_reserved_0) << R95_RESERVED_0_OFF) & R95_RESERVED_0_MSK))
// Register R96 [0x60] -- R96

enum r96_fields_t {
    R96_RESERVED_0_OFF = 0x0,
    R96_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R96(r96_reserved_0) MAKE_LMX2820_REG_WR(R96, \
    (((r96_reserved_0) << R96_RESERVED_0_OFF) & R96_RESERVED_0_MSK))
// Register R97 [0x61] -- R97

enum r97_fields_t {
    R97_RESERVED_0_OFF = 0x0,
    R97_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R97(r97_reserved_0) MAKE_LMX2820_REG_WR(R97, \
    (((r97_reserved_0) << R97_RESERVED_0_OFF) & R97_RESERVED_0_MSK))
// Register R98 [0x62] -- R98

enum r98_fields_t {
    R98_RESERVED_0_OFF = 0x0,
    R98_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R98(r98_reserved_0) MAKE_LMX2820_REG_WR(R98, \
    (((r98_reserved_0) << R98_RESERVED_0_OFF) & R98_RESERVED_0_MSK))
// Register R99 [0x63] -- R99

enum r99_fields_t {
    R99_RESERVED_0_OFF = 0x0,
    R99_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R99(r99_reserved_0) MAKE_LMX2820_REG_WR(R99, \
    (((r99_reserved_0) << R99_RESERVED_0_OFF) & R99_RESERVED_0_MSK))
// Register R100 [0x64] -- R100

enum r100_fields_t {
    R100_RESERVED_0_OFF = 0x0,
    R100_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R100(r100_reserved_0) MAKE_LMX2820_REG_WR(R100, \
    (((r100_reserved_0) << R100_RESERVED_0_OFF) & R100_RESERVED_0_MSK))
// Register R101 [0x65] -- R101

enum r101_fields_t {
    R101_RESERVED_0_OFF = 0x0,
    R101_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R101(r101_reserved_0) MAKE_LMX2820_REG_WR(R101, \
    (((r101_reserved_0) << R101_RESERVED_0_OFF) & R101_RESERVED_0_MSK))
// Register R102 [0x66] -- R102

enum r102_fields_t {
    R102_RESERVED_0_OFF = 0x0,
    R102_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R102(r102_reserved_0) MAKE_LMX2820_REG_WR(R102, \
    (((r102_reserved_0) << R102_RESERVED_0_OFF) & R102_RESERVED_0_MSK))
// Register R103 [0x67] -- R103

enum r103_fields_t {
    R103_RESERVED_0_OFF = 0x0,
    R103_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R103(r103_reserved_0) MAKE_LMX2820_REG_WR(R103, \
    (((r103_reserved_0) << R103_RESERVED_0_OFF) & R103_RESERVED_0_MSK))
// Register R104 [0x68] -- R104

enum r104_fields_t {
    R104_RESERVED_0_OFF = 0x0,
    R104_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R104(r104_reserved_0) MAKE_LMX2820_REG_WR(R104, \
    (((r104_reserved_0) << R104_RESERVED_0_OFF) & R104_RESERVED_0_MSK))
// Register R105 [0x69] -- R105

enum r105_fields_t {
    R105_RESERVED_0_OFF = 0x0,
    R105_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R105(r105_reserved_0) MAKE_LMX2820_REG_WR(R105, \
    (((r105_reserved_0) << R105_RESERVED_0_OFF) & R105_RESERVED_0_MSK))
// Register R106 [0x6a] -- R106

enum r106_fields_t {
    R106_RESERVED_0_OFF = 0x0,
    R106_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R106(r106_reserved_0) MAKE_LMX2820_REG_WR(R106, \
    (((r106_reserved_0) << R106_RESERVED_0_OFF) & R106_RESERVED_0_MSK))
// Register R107 [0x6b] -- R107

enum r107_fields_t {
    R107_RESERVED_0_OFF = 0x0,
    R107_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R107(r107_reserved_0) MAKE_LMX2820_REG_WR(R107, \
    (((r107_reserved_0) << R107_RESERVED_0_OFF) & R107_RESERVED_0_MSK))
// Register R108 [0x6c] -- R108

enum r108_fields_t {
    R108_RESERVED_0_OFF = 0x0,
    R108_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R108(r108_reserved_0) MAKE_LMX2820_REG_WR(R108, \
    (((r108_reserved_0) << R108_RESERVED_0_OFF) & R108_RESERVED_0_MSK))
// Register R109 [0x6d] -- R109

enum r109_fields_t {
    R109_RESERVED_0_OFF = 0x0,
    R109_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R109(r109_reserved_0) MAKE_LMX2820_REG_WR(R109, \
    (((r109_reserved_0) << R109_RESERVED_0_OFF) & R109_RESERVED_0_MSK))
// Register R110 [0x6e] -- R110

enum r110_fields_t {
    R110_RESERVED_0_OFF = 0x0,
    R110_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R110(r110_reserved_0) MAKE_LMX2820_REG_WR(R110, \
    (((r110_reserved_0) << R110_RESERVED_0_OFF) & R110_RESERVED_0_MSK))
// Register R111 [0x6f] -- R111

enum r111_fields_t {
    R111_RESERVED_0_OFF = 0x0,
    R111_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R111(r111_reserved_0) MAKE_LMX2820_REG_WR(R111, \
    (((r111_reserved_0) << R111_RESERVED_0_OFF) & R111_RESERVED_0_MSK))
// Register R112 [0x70] -- R112

enum r112_fields_t {
    R112_RESERVED_0_OFF = 0x0,
    R112_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R112(r112_reserved_0) MAKE_LMX2820_REG_WR(R112, \
    (((r112_reserved_0) << R112_RESERVED_0_OFF) & R112_RESERVED_0_MSK))
// Register R113 [0x71] -- R113

enum r113_fields_t {
    R113_RESERVED_0_OFF = 0x0,
    R113_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R113(r113_reserved_0) MAKE_LMX2820_REG_WR(R113, \
    (((r113_reserved_0) << R113_RESERVED_0_OFF) & R113_RESERVED_0_MSK))
// Register R114 [0x72] -- R114

enum r114_fields_t {
    R114_RESERVED_0_OFF = 0x0,
    R114_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R114(r114_reserved_0) MAKE_LMX2820_REG_WR(R114, \
    (((r114_reserved_0) << R114_RESERVED_0_OFF) & R114_RESERVED_0_MSK))
// Register R115 [0x73] -- R115

enum r115_fields_t {
    R115_RESERVED_0_OFF = 0x0,
    R115_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R115(r115_reserved_0) MAKE_LMX2820_REG_WR(R115, \
    (((r115_reserved_0) << R115_RESERVED_0_OFF) & R115_RESERVED_0_MSK))
// Register R116 [0x74] -- R116

enum r116_fields_t {
    R116_RESERVED_0_OFF = 0x0,
    R116_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R116(r116_reserved_0) MAKE_LMX2820_REG_WR(R116, \
    (((r116_reserved_0) << R116_RESERVED_0_OFF) & R116_RESERVED_0_MSK))
// Register R117 [0x75] -- R117

enum r117_fields_t {
    R117_RESERVED_0_OFF = 0x0,
    R117_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R117(r117_reserved_0) MAKE_LMX2820_REG_WR(R117, \
    (((r117_reserved_0) << R117_RESERVED_0_OFF) & R117_RESERVED_0_MSK))
// Register R118 [0x76] -- R118

enum r118_fields_t {
    R118_RESERVED_0_OFF = 0x0,
    R118_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R118(r118_reserved_0) MAKE_LMX2820_REG_WR(R118, \
    (((r118_reserved_0) << R118_RESERVED_0_OFF) & R118_RESERVED_0_MSK))
// Register R119 [0x77] -- R119

enum r119_fields_t {
    R119_RESERVED_0_OFF = 0x0,
    R119_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R119(r119_reserved_0) MAKE_LMX2820_REG_WR(R119, \
    (((r119_reserved_0) << R119_RESERVED_0_OFF) & R119_RESERVED_0_MSK))
// Register R120 [0x78] -- R120

enum r120_fields_t {
    R120_RESERVED_0_OFF = 0x0,
    R120_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R120(r120_reserved_0) MAKE_LMX2820_REG_WR(R120, \
    (((r120_reserved_0) << R120_RESERVED_0_OFF) & R120_RESERVED_0_MSK))
// Register R121 [0x79] -- R121

enum r121_fields_t {
    R121_RESERVED_0_OFF = 0x0,
    R121_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R121(r121_reserved_0) MAKE_LMX2820_REG_WR(R121, \
    (((r121_reserved_0) << R121_RESERVED_0_OFF) & R121_RESERVED_0_MSK))
// Register R122 [0x7a] -- R122

enum r122_fields_t {
    R122_RESERVED_0_OFF = 0x0,
    R122_RESERVED_0_MSK = 0xffff,
};
#define MAKE_LMX2820_R122(r122_reserved_0) MAKE_LMX2820_REG_WR(R122, \
    (((r122_reserved_0) << R122_RESERVED_0_OFF) & R122_RESERVED_0_MSK))
