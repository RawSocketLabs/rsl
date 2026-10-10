enum lp875484_regs_t {
    VSET_B0 = 0x0,
    FPWM = 0x6,
    FLAGS = 0xd,
    RESET = 0x11,
    CHIP_ID = 0x18,
    SEL_I_LOAD = 0x21,
    LOAD_CURR_LO = 0x22,
};
#define MAKE_LP875484_REG_WR(a, v) (0x8000 | ((a) << 8) | ((v) & 0xff))
#define MAKE_LP875484_REG_RD(a) (((a) << 8))
// Register R0 [0x0] -- VSET_B0

enum vset_b0_fields_t {
    EN_DIS_B0_OFF = 0x7,
    EN_DIS_B0_MSK = 0x80,
    VSET_B0_OFF = 0x0,
    VSET_B0_MSK = 0x7f,
};
#define GET_LP875484_EN_DIS_B0(x) (((x) & EN_DIS_B0_MSK) >> EN_DIS_B0_OFF)
#define GET_LP875484_VSET_B0(x) (((x) & VSET_B0_MSK) >> VSET_B0_OFF)
#define SET_LP875484_EN_DIS_B0(p, f) (p) = ((p) & ~EN_DIS_B0_MSK) | (((f) << EN_DIS_B0_OFF) & EN_DIS_B0_MSK)
#define SET_LP875484_VSET_B0(p, f) (p) = ((p) & ~VSET_B0_MSK) | (((f) << VSET_B0_OFF) & VSET_B0_MSK)

#define MAKE_LP875484_VSET_B0(en_dis_b0, vset_b0) MAKE_LP875484_REG_WR(VSET_B0, \
    (((en_dis_b0) << EN_DIS_B0_OFF) & EN_DIS_B0_MSK) |  \
    (((vset_b0) << VSET_B0_OFF) & VSET_B0_MSK))
// Register R6 [0x6] -- FPWM

enum fpwm_fields_t {
    RESERVED_OFF = 0x1,
    RESERVED_MSK = 0xfe,
    FPWM_B0_OFF = 0x0,
    FPWM_B0_MSK = 0x1,
};
#define GET_LP875484_RESERVED(x) (((x) & RESERVED_MSK) >> RESERVED_OFF)
#define GET_LP875484_FPWM_B0(x) (((x) & FPWM_B0_MSK) >> FPWM_B0_OFF)
#define SET_LP875484_RESERVED(p, f) (p) = ((p) & ~RESERVED_MSK) | (((f) << RESERVED_OFF) & RESERVED_MSK)
#define SET_LP875484_FPWM_B0(p, f) (p) = ((p) & ~FPWM_B0_MSK) | (((f) << FPWM_B0_OFF) & FPWM_B0_MSK)

#define MAKE_LP875484_FPWM(reserved, fpwm_b0) MAKE_LP875484_REG_WR(FPWM, \
    (((reserved) << RESERVED_OFF) & RESERVED_MSK) |  \
    (((fpwm_b0) << FPWM_B0_OFF) & FPWM_B0_MSK))
// Register R13 [0xd] -- FLAGS

enum flags_fields_t {
    N_PG_B0_OFF = 0x2,
    N_PG_B0_MSK = 0x4,
    TEMP_OFF = 0x0,
    TEMP_MSK = 0x3,
};
#define GET_LP875484_N_PG_B0(x) (((x) & N_PG_B0_MSK) >> N_PG_B0_OFF)
#define GET_LP875484_TEMP(x) (((x) & TEMP_MSK) >> TEMP_OFF)
#define SET_LP875484_N_PG_B0(p, f) (p) = ((p) & ~N_PG_B0_MSK) | (((f) << N_PG_B0_OFF) & N_PG_B0_MSK)
#define SET_LP875484_TEMP(p, f) (p) = ((p) & ~TEMP_MSK) | (((f) << TEMP_OFF) & TEMP_MSK)

#define MAKE_LP875484_FLAGS(n_pg_b0, temp) MAKE_LP875484_REG_WR(FLAGS, \
    (((n_pg_b0) << N_PG_B0_OFF) & N_PG_B0_MSK) |  \
    (((temp) << TEMP_OFF) & TEMP_MSK))
// Register R17 [0x11] -- RESET

// Register R24 [0x18] -- CHIP_ID

// Register R33 [0x21] -- SEL_I_LOAD
enum load_current_source_options {
    LOAD_CURRENT_SOURCE_CONV_0 = 0,
    LOAD_CURRENT_SOURCE_CONV_1 = 1,
    LOAD_CURRENT_SOURCE_CONV_2 = 2,
    LOAD_CURRENT_SOURCE_CONV_3 = 3,
    LOAD_CURRENT_SOURCE_CONV_4 = 4,
    LOAD_CURRENT_SOURCE_CONV_5 = 5,
    LOAD_CURRENT_SOURCE_CONV_ALL = 6,
    LOAD_CURRENT_SOURCE_CONV_RESERVED = 7,
};

enum sel_i_load_fields_t {
    BUCK_LOAD_CURR_HI_OFF = 0x4,
    BUCK_LOAD_CURR_HI_MSK = 0x70,
    LOAD_CURRENT_SOURCE_OFF = 0x0,
    LOAD_CURRENT_SOURCE_MSK = 0x7,
};
#define GET_LP875484_BUCK_LOAD_CURR_HI(x) (((x) & BUCK_LOAD_CURR_HI_MSK) >> BUCK_LOAD_CURR_HI_OFF)
#define GET_LP875484_LOAD_CURRENT_SOURCE(x) (((x) & LOAD_CURRENT_SOURCE_MSK) >> LOAD_CURRENT_SOURCE_OFF)
#define SET_LP875484_BUCK_LOAD_CURR_HI(p, f) (p) = ((p) & ~BUCK_LOAD_CURR_HI_MSK) | (((f) << BUCK_LOAD_CURR_HI_OFF) & BUCK_LOAD_CURR_HI_MSK)
#define SET_LP875484_LOAD_CURRENT_SOURCE(p, f) (p) = ((p) & ~LOAD_CURRENT_SOURCE_MSK) | (((f) << LOAD_CURRENT_SOURCE_OFF) & LOAD_CURRENT_SOURCE_MSK)

#define MAKE_LP875484_SEL_I_LOAD(buck_load_curr_hi, load_current_source) MAKE_LP875484_REG_WR(SEL_I_LOAD, \
    (((buck_load_curr_hi) << BUCK_LOAD_CURR_HI_OFF) & BUCK_LOAD_CURR_HI_MSK) |  \
    (((load_current_source) << LOAD_CURRENT_SOURCE_OFF) & LOAD_CURRENT_SOURCE_MSK))
// Register R34 [0x22] -- LOAD_CURR_LO

