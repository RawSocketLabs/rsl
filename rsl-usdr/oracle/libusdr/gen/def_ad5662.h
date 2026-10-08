enum ad5662_regs_t {
    OUTPUT = 0x0,
};
#define MAKE_AD5662_REG_WR(a, v) (0x80000000 | ((a) << 24) | ((v) & 0xffffff))
#define MAKE_AD5662_REG_RD(a) (((a) << 24))
// Register R0 [0x0] -- OUTPUT
enum power_down_options {
    POWER_DOWN_NORMAL_OPERATION = 0,
    POWER_DOWN_1KOMH_TO_GND = 1,
    POWER_DOWN_100KOHM_TO_GND = 2,
    POWER_DOWN_TREE_STATE = 3,
};

enum output_fields_t {
    OUTPUT_OFF = 0x8,
    OUTPUT_MSK = 0xffff00,
    POWER_DOWN_OFF = 0x6,
    POWER_DOWN_MSK = 0xc0,
    NOT_USED_OFF = 0x0,
    NOT_USED_MSK = 0x3f,
};
#define MAKE_AD5662_OUTPUT(output, power_down, not_used) MAKE_AD5662_REG_WR(OUTPUT, \
    (((output) << OUTPUT_OFF) & OUTPUT_MSK) |  \
    (((power_down) << POWER_DOWN_OFF) & POWER_DOWN_MSK) |  \
    (((not_used) << NOT_USED_OFF) & NOT_USED_MSK))
