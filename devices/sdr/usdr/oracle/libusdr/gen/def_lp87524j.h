enum lp87524j_regs_t {
    BUCK1_CTRL1 = 0x4,
};
#define MAKE_LP87524J_REG_WR(a, v) (0x80000000 | ((a) << 16) | ((v) & 0xffff))
#define MAKE_LP87524J_REG_RD(a) (((a) << 16))
// Register R4 [0x4] -- BUCK1_CTRL1
enum en_buck1_options {
    EN_BUCK1_BUCK1_REGULATOR_IS_DISABLED = 0,
    EN_BUCK1_BUCK1_REGULATOR_IS_ENABLED = 1,
};
enum en_pin_ctrl1_options {
    EN_PIN_CTRL1_ONLY_EN_BUCK1_BIT_CONTROLS_BUCK1 = 0,
    EN_PIN_CTRL1_EN_BUCK1_BIT_AND_ENX_PIN_CONTROL_BUCK1 = 1,
};
enum buck1_en_pin_select_options {
    BUCK1_EN_PIN_SELECT_EN_BUCK1_BIT_AND_EN1_PIN_CONTROL_BUCK1 = 0,
    BUCK1_EN_PIN_SELECT_EN_BUCK1_BIT_AND_EN2_PIN_CONTROL_BUCK1 = 1,
    BUCK1_EN_PIN_SELECT_EN_BUCK1_BIT_AND_EN3_PIN_CONTROL_BUCK1 = 2,
    BUCK1_EN_PIN_SELECT_RESERVED = 3,
};
enum en_roof_floor1_options {
    EN_ROOF_FLOOR1_ENABLEDIVDISABLE_1DIV0_CONTROL = 0,
    EN_ROOF_FLOOR1_ROOFDIVFLOOR_1DIV0_CONTROL = 1,
};
enum en_rdis1_options {
    EN_RDIS1_DISCHARGE_RESISTOR_DISABLED = 0,
    EN_RDIS1_DISCHARGE_RESISTOR_ENABLED = 1,
};
enum buck1_fpwm_options {
    BUCK1_FPWM_AUTOMATIC_TRANSITIONS_BETWEEN_PFM_AND_PWM_MODES_AUTO_MODE_ = 0,
    BUCK1_FPWM_FORCED_TO_PWM_OPERATION = 1,
};

enum buck1_ctrl1_fields_t {
    EN_BUCK1_OFF = 0x7,
    EN_BUCK1_MSK = 0x80,
    EN_PIN_CTRL1_OFF = 0x6,
    EN_PIN_CTRL1_MSK = 0x40,
    BUCK1_EN_PIN_SELECT_OFF = 0x4,
    BUCK1_EN_PIN_SELECT_MSK = 0x30,
    EN_ROOF_FLOOR1_OFF = 0x3,
    EN_ROOF_FLOOR1_MSK = 0x8,
    EN_RDIS1_OFF = 0x2,
    EN_RDIS1_MSK = 0x4,
    BUCK1_FPWM_OFF = 0x1,
    BUCK1_FPWM_MSK = 0x2,
    BUCK1_CTRL1_RESERVED_0_OFF = 0x0,
    BUCK1_CTRL1_RESERVED_0_MSK = 0x1,
};
#define MAKE_LP87524J_BUCK1_CTRL1(en_buck1, en_pin_ctrl1, buck1_en_pin_select, en_roof_floor1, en_rdis1, buck1_fpwm, buck1_ctrl1_reserved_0) MAKE_LP87524J_REG_WR(BUCK1_CTRL1, \
    (((en_buck1) << EN_BUCK1_OFF) & EN_BUCK1_MSK) |  \
    (((en_pin_ctrl1) << EN_PIN_CTRL1_OFF) & EN_PIN_CTRL1_MSK) |  \
    (((buck1_en_pin_select) << BUCK1_EN_PIN_SELECT_OFF) & BUCK1_EN_PIN_SELECT_MSK) |  \
    (((en_roof_floor1) << EN_ROOF_FLOOR1_OFF) & EN_ROOF_FLOOR1_MSK) |  \
    (((en_rdis1) << EN_RDIS1_OFF) & EN_RDIS1_MSK) |  \
    (((buck1_fpwm) << BUCK1_FPWM_OFF) & BUCK1_FPWM_MSK) |  \
    (((buck1_ctrl1_reserved_0) << BUCK1_CTRL1_RESERVED_0_OFF) & BUCK1_CTRL1_RESERVED_0_MSK))
