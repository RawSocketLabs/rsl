enum lmk1d1208i_regs_t {
    R0 = 0x0,
    R1 = 0x1,
    R2 = 0x2,
    R5 = 0x5,
    R14 = 0xe,
};
#define MAKE_LMK1D1208I_REG_WR(a, v) (((a) << 8) | ((v) & 0xff))
#define MAKE_LMK1D1208I_REG_RD(a) (((a) << 8))
// Register R0 [0x0] -- R0
enum out7_en_options {
    OUT7_EN_OUTPUT_DISABLED_HI_Z = 0,
    OUT7_EN_OUTPUT_ENABLED = 1,
};
enum out6_en_options {
    OUT6_EN_OUTPUT_DISABLED_HI_Z = 0,
    OUT6_EN_OUTPUT_ENABLED = 1,
};
enum out5_en_options {
    OUT5_EN_OUTPUT_DISABLED_HI_Z = 0,
    OUT5_EN_OUTPUT_ENABLED = 1,
};
enum out4_en_options {
    OUT4_EN_OUTPUT_DISABLED_HI_Z = 0,
    OUT4_EN_OUTPUT_ENABLED = 1,
};
enum out3_en_options {
    OUT3_EN_OUTPUT_DISABLED_HI_Z = 0,
    OUT3_EN_OUTPUT_ENABLED = 1,
};
enum out2_en_options {
    OUT2_EN_OUTPUT_DISABLED_HI_Z = 0,
    OUT2_EN_OUTPUT_ENABLED = 1,
};
enum out1_en_options {
    OUT1_EN_OUTPUT_DISABLED_HI_Z = 0,
    OUT1_EN_OUTPUT_ENABLED = 1,
};
enum out0_en_options {
    OUT0_EN_OUTPUT_DISABLED_HI_Z = 0,
    OUT0_EN_OUTPUT_ENABLED = 1,
};

enum r0_fields_t {
    OUT7_EN_OFF = 0x7,
    OUT7_EN_MSK = 0x80,
    OUT6_EN_OFF = 0x6,
    OUT6_EN_MSK = 0x40,
    OUT5_EN_OFF = 0x5,
    OUT5_EN_MSK = 0x20,
    OUT4_EN_OFF = 0x4,
    OUT4_EN_MSK = 0x10,
    OUT3_EN_OFF = 0x3,
    OUT3_EN_MSK = 0x8,
    OUT2_EN_OFF = 0x2,
    OUT2_EN_MSK = 0x4,
    OUT1_EN_OFF = 0x1,
    OUT1_EN_MSK = 0x2,
    OUT0_EN_OFF = 0x0,
    OUT0_EN_MSK = 0x1,
};
#define MAKE_LMK1D1208I_R0(out7_en, out6_en, out5_en, out4_en, out3_en, out2_en, out1_en, out0_en) MAKE_LMK1D1208I_REG_WR(R0, \
    (((out7_en) << OUT7_EN_OFF) & OUT7_EN_MSK) |  \
    (((out6_en) << OUT6_EN_OFF) & OUT6_EN_MSK) |  \
    (((out5_en) << OUT5_EN_OFF) & OUT5_EN_MSK) |  \
    (((out4_en) << OUT4_EN_OFF) & OUT4_EN_MSK) |  \
    (((out3_en) << OUT3_EN_OFF) & OUT3_EN_MSK) |  \
    (((out2_en) << OUT2_EN_OFF) & OUT2_EN_MSK) |  \
    (((out1_en) << OUT1_EN_OFF) & OUT1_EN_MSK) |  \
    (((out0_en) << OUT0_EN_OFF) & OUT0_EN_MSK))
// Register R1 [0x1] -- R1
enum out7_amp_sel_options {
    OUT7_AMP_SEL_STANDARD_LVDS_SWING_350_MV = 0,
    OUT7_AMP_SEL_BOOSTED_LVDS_SWING_500_MV = 1,
};
enum out6_amp_sel_options {
    OUT6_AMP_SEL_STANDARD_LVDS_SWING_350_MV = 0,
    OUT6_AMP_SEL_BOOSTED_LVDS_SWING_500_MV = 1,
};
enum out5_amp_sel_options {
    OUT5_AMP_SEL_STANDARD_LVDS_SWING_350_MV = 0,
    OUT5_AMP_SEL_BOOSTED_LVDS_SWING_500_MV = 1,
};
enum out4_amp_sel_options {
    OUT4_AMP_SEL_STANDARD_LVDS_SWING_350_MV = 0,
    OUT4_AMP_SEL_BOOSTED_LVDS_SWING_500_MV = 1,
};
enum out3_amp_sel_options {
    OUT3_AMP_SEL_STANDARD_LVDS_SWING_350_MV = 0,
    OUT3_AMP_SEL_BOOSTED_LVDS_SWING_500_MV = 1,
};
enum out2_amp_sel_options {
    OUT2_AMP_SEL_STANDARD_LVDS_SWING_350_MV = 0,
    OUT2_AMP_SEL_BOOSTED_LVDS_SWING_500_MV = 1,
};
enum out1_amp_sel_options {
    OUT1_AMP_SEL_STANDARD_LVDS_SWING_350_MV = 0,
    OUT1_AMP_SEL_BOOSTED_LVDS_SWING_500_MV = 1,
};
enum out0_amp_sel_options {
    OUT0_AMP_SEL_STANDARD_LVDS_SWING_350_MV = 0,
    OUT0_AMP_SEL_BOOSTED_LVDS_SWING_500_MV = 1,
};

enum r1_fields_t {
    OUT7_AMP_SEL_OFF = 0x7,
    OUT7_AMP_SEL_MSK = 0x80,
    OUT6_AMP_SEL_OFF = 0x6,
    OUT6_AMP_SEL_MSK = 0x40,
    OUT5_AMP_SEL_OFF = 0x5,
    OUT5_AMP_SEL_MSK = 0x20,
    OUT4_AMP_SEL_OFF = 0x4,
    OUT4_AMP_SEL_MSK = 0x10,
    OUT3_AMP_SEL_OFF = 0x3,
    OUT3_AMP_SEL_MSK = 0x8,
    OUT2_AMP_SEL_OFF = 0x2,
    OUT2_AMP_SEL_MSK = 0x4,
    OUT1_AMP_SEL_OFF = 0x1,
    OUT1_AMP_SEL_MSK = 0x2,
    OUT0_AMP_SEL_OFF = 0x0,
    OUT0_AMP_SEL_MSK = 0x1,
};
#define MAKE_LMK1D1208I_R1(out7_amp_sel, out6_amp_sel, out5_amp_sel, out4_amp_sel, out3_amp_sel, out2_amp_sel, out1_amp_sel, out0_amp_sel) MAKE_LMK1D1208I_REG_WR(R1, \
    (((out7_amp_sel) << OUT7_AMP_SEL_OFF) & OUT7_AMP_SEL_MSK) |  \
    (((out6_amp_sel) << OUT6_AMP_SEL_OFF) & OUT6_AMP_SEL_MSK) |  \
    (((out5_amp_sel) << OUT5_AMP_SEL_OFF) & OUT5_AMP_SEL_MSK) |  \
    (((out4_amp_sel) << OUT4_AMP_SEL_OFF) & OUT4_AMP_SEL_MSK) |  \
    (((out3_amp_sel) << OUT3_AMP_SEL_OFF) & OUT3_AMP_SEL_MSK) |  \
    (((out2_amp_sel) << OUT2_AMP_SEL_OFF) & OUT2_AMP_SEL_MSK) |  \
    (((out1_amp_sel) << OUT1_AMP_SEL_OFF) & OUT1_AMP_SEL_MSK) |  \
    (((out0_amp_sel) << OUT0_AMP_SEL_OFF) & OUT0_AMP_SEL_MSK))
// Register R2 [0x2] -- R2
enum bank1_in_sel_options {
    BANK1_IN_SEL_IN1_PDIVIN1_N = 0,
    BANK1_IN_SEL_IN0_PDIVIN0_N = 1,
};
enum bank0_in_sel_options {
    BANK0_IN_SEL_IN1_PDIVIN1_N = 0,
    BANK0_IN_SEL_IN0_PDIVIN0_N = 1,
};
enum bank1_mute_options {
    BANK1_MUTE_INX_PDIVINX_N = 0,
    BANK1_MUTE_LOGIC_LOW = 1,
};
enum bank0_mute_options {
    BANK0_MUTE_INX_PDIVINX_N = 0,
    BANK0_MUTE_LOGIC_LOW = 1,
};
enum in1_en_options {
    IN1_EN_INPUT_DISABLED_REDUCES_POWER_CONSUMPTION = 0,
    IN1_EN_INPUT_ENABLED = 1,
};
enum in0_en_options {
    IN0_EN_INPUT_DISABLED_REDUCES_POWER_CONSUMPTION = 0,
    IN0_EN_INPUT_ENABLED = 1,
};

enum r2_fields_t {
    R2_RESERVED_0_OFF = 0x7,
    R2_RESERVED_0_MSK = 0x80,
    R2_RESERVED_1_OFF = 0x6,
    R2_RESERVED_1_MSK = 0x40,
    BANK1_IN_SEL_OFF = 0x5,
    BANK1_IN_SEL_MSK = 0x20,
    BANK0_IN_SEL_OFF = 0x4,
    BANK0_IN_SEL_MSK = 0x10,
    BANK1_MUTE_OFF = 0x3,
    BANK1_MUTE_MSK = 0x8,
    BANK0_MUTE_OFF = 0x2,
    BANK0_MUTE_MSK = 0x4,
    IN1_EN_OFF = 0x1,
    IN1_EN_MSK = 0x2,
    IN0_EN_OFF = 0x0,
    IN0_EN_MSK = 0x1,
};
#define MAKE_LMK1D1208I_R2(r2_reserved_0, r2_reserved_1, bank1_in_sel, bank0_in_sel, bank1_mute, bank0_mute, in1_en, in0_en) MAKE_LMK1D1208I_REG_WR(R2, \
    (((r2_reserved_0) << R2_RESERVED_0_OFF) & R2_RESERVED_0_MSK) |  \
    (((r2_reserved_1) << R2_RESERVED_1_OFF) & R2_RESERVED_1_MSK) |  \
    (((bank1_in_sel) << BANK1_IN_SEL_OFF) & BANK1_IN_SEL_MSK) |  \
    (((bank0_in_sel) << BANK0_IN_SEL_OFF) & BANK0_IN_SEL_MSK) |  \
    (((bank1_mute) << BANK1_MUTE_OFF) & BANK1_MUTE_MSK) |  \
    (((bank0_mute) << BANK0_MUTE_OFF) & BANK0_MUTE_MSK) |  \
    (((in1_en) << IN1_EN_OFF) & IN1_EN_MSK) |  \
    (((in0_en) << IN0_EN_OFF) & IN0_EN_MSK))
// Register R5 [0x5] -- R5

enum r5_fields_t {
    REV_ID_OFF = 0x4,
    REV_ID_MSK = 0xf0,
    DEV_ID_OFF = 0x0,
    DEV_ID_MSK = 0xf,
};
#define MAKE_LMK1D1208I_R5(rev_id, dev_id) MAKE_LMK1D1208I_REG_WR(R5, \
    (((rev_id) << REV_ID_OFF) & REV_ID_MSK) |  \
    (((dev_id) << DEV_ID_OFF) & DEV_ID_MSK))
// Register R14 [0xe] -- R14

enum r14_fields_t {
    IDX_RB_OFF = 0x0,
    IDX_RB_MSK = 0xff,
};
#define MAKE_LMK1D1208I_R14(idx_rb) MAKE_LMK1D1208I_REG_WR(R14, \
    (((idx_rb) << IDX_RB_OFF) & IDX_RB_MSK))
