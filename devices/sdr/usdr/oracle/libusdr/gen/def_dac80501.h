enum dac80501_regs_t {
    DEVID = 0x1,
    SYNC = 0x2,
    CONFIG = 0x3,
    GAIN = 0x4,
    TRIGGER = 0x5,
    STATUS = 0x7,
    DAC = 0x8,
};
#define MAKE_DAC80501_REG_WR(a, v) (0x80000000 | ((a) << 16) | ((v) & 0xffff))
#define MAKE_DAC80501_REG_RD(a) (((a) << 16))
// Register R1 [0x1] -- DEVID

enum devid_fields_t {
    RESOLUTION_OFF = 0xc,
    RESOLUTION_MSK = 0x7000,
    RSTSEL_OFF = 0x7,
    RSTSEL_MSK = 0x80,
};
#define GET_DAC80501_RESOLUTION(x) (((x) & RESOLUTION_MSK) >> RESOLUTION_OFF)
#define GET_DAC80501_RSTSEL(x) (((x) & RSTSEL_MSK) >> RSTSEL_OFF)
#define SET_DAC80501_RESOLUTION(p, f) (p) = ((p) & ~RESOLUTION_MSK) | (((f) << RESOLUTION_OFF) & RESOLUTION_MSK)
#define SET_DAC80501_RSTSEL(p, f) (p) = ((p) & ~RSTSEL_MSK) | (((f) << RSTSEL_OFF) & RSTSEL_MSK)

#define MAKE_DAC80501_DEVID(resolution, rstsel) MAKE_DAC80501_REG_WR(DEVID, \
    (((resolution) << RESOLUTION_OFF) & RESOLUTION_MSK) |  \
    (((rstsel) << RSTSEL_OFF) & RSTSEL_MSK))
// Register R2 [0x2] -- SYNC

enum sync_fields_t {
    DAC_SYNC_EN_OFF = 0x0,
    DAC_SYNC_EN_MSK = 0x1,
};
#define GET_DAC80501_DAC_SYNC_EN(x) (((x) & DAC_SYNC_EN_MSK) >> DAC_SYNC_EN_OFF)
#define SET_DAC80501_DAC_SYNC_EN(p, f) (p) = ((p) & ~DAC_SYNC_EN_MSK) | (((f) << DAC_SYNC_EN_OFF) & DAC_SYNC_EN_MSK)

#define MAKE_DAC80501_SYNC(dac_sync_en) MAKE_DAC80501_REG_WR(SYNC, \
    (((dac_sync_en) << DAC_SYNC_EN_OFF) & DAC_SYNC_EN_MSK))
// Register R3 [0x3] -- CONFIG

enum config_fields_t {
    REF_PWDWN_OFF = 0x8,
    REF_PWDWN_MSK = 0x100,
    DAC_PWDWN_OFF = 0x0,
    DAC_PWDWN_MSK = 0x1,
};
#define GET_DAC80501_REF_PWDWN(x) (((x) & REF_PWDWN_MSK) >> REF_PWDWN_OFF)
#define GET_DAC80501_DAC_PWDWN(x) (((x) & DAC_PWDWN_MSK) >> DAC_PWDWN_OFF)
#define SET_DAC80501_REF_PWDWN(p, f) (p) = ((p) & ~REF_PWDWN_MSK) | (((f) << REF_PWDWN_OFF) & REF_PWDWN_MSK)
#define SET_DAC80501_DAC_PWDWN(p, f) (p) = ((p) & ~DAC_PWDWN_MSK) | (((f) << DAC_PWDWN_OFF) & DAC_PWDWN_MSK)

#define MAKE_DAC80501_CONFIG(ref_pwdwn, dac_pwdwn) MAKE_DAC80501_REG_WR(CONFIG, \
    (((ref_pwdwn) << REF_PWDWN_OFF) & REF_PWDWN_MSK) |  \
    (((dac_pwdwn) << DAC_PWDWN_OFF) & DAC_PWDWN_MSK))
// Register R4 [0x4] -- GAIN

enum gain_fields_t {
    REF_DIV_OFF = 0x8,
    REF_DIV_MSK = 0x100,
    BUFF_GAIN_OFF = 0x0,
    BUFF_GAIN_MSK = 0x1,
};
#define GET_DAC80501_REF_DIV(x) (((x) & REF_DIV_MSK) >> REF_DIV_OFF)
#define GET_DAC80501_BUFF_GAIN(x) (((x) & BUFF_GAIN_MSK) >> BUFF_GAIN_OFF)
#define SET_DAC80501_REF_DIV(p, f) (p) = ((p) & ~REF_DIV_MSK) | (((f) << REF_DIV_OFF) & REF_DIV_MSK)
#define SET_DAC80501_BUFF_GAIN(p, f) (p) = ((p) & ~BUFF_GAIN_MSK) | (((f) << BUFF_GAIN_OFF) & BUFF_GAIN_MSK)

#define MAKE_DAC80501_GAIN(ref_div, buff_gain) MAKE_DAC80501_REG_WR(GAIN, \
    (((ref_div) << REF_DIV_OFF) & REF_DIV_MSK) |  \
    (((buff_gain) << BUFF_GAIN_OFF) & BUFF_GAIN_MSK))
// Register R5 [0x5] -- TRIGGER

enum trigger_fields_t {
    LDAC_OFF = 0x4,
    LDAC_MSK = 0x10,
    SOFT_RESET_OFF = 0x0,
    SOFT_RESET_MSK = 0xf,
};
#define GET_DAC80501_LDAC(x) (((x) & LDAC_MSK) >> LDAC_OFF)
#define GET_DAC80501_SOFT_RESET(x) (((x) & SOFT_RESET_MSK) >> SOFT_RESET_OFF)
#define SET_DAC80501_LDAC(p, f) (p) = ((p) & ~LDAC_MSK) | (((f) << LDAC_OFF) & LDAC_MSK)
#define SET_DAC80501_SOFT_RESET(p, f) (p) = ((p) & ~SOFT_RESET_MSK) | (((f) << SOFT_RESET_OFF) & SOFT_RESET_MSK)

#define MAKE_DAC80501_TRIGGER(ldac, soft_reset) MAKE_DAC80501_REG_WR(TRIGGER, \
    (((ldac) << LDAC_OFF) & LDAC_MSK) |  \
    (((soft_reset) << SOFT_RESET_OFF) & SOFT_RESET_MSK))
// Register R7 [0x7] -- STATUS

enum status_fields_t {
    REF_ALARM_OFF = 0x0,
    REF_ALARM_MSK = 0x1,
};
#define GET_DAC80501_REF_ALARM(x) (((x) & REF_ALARM_MSK) >> REF_ALARM_OFF)
#define SET_DAC80501_REF_ALARM(p, f) (p) = ((p) & ~REF_ALARM_MSK) | (((f) << REF_ALARM_OFF) & REF_ALARM_MSK)

#define MAKE_DAC80501_STATUS(ref_alarm) MAKE_DAC80501_REG_WR(STATUS, \
    (((ref_alarm) << REF_ALARM_OFF) & REF_ALARM_MSK))
// Register R8 [0x8] -- DAC

