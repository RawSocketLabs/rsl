enum tps6381x_regs_t {
    CONTROL = 0x1,
    STATUS = 0x2,
    DEVID = 0x3,
    VOUT1 = 0x4,
    VOUT2 = 0x5,
};
#define MAKE_TPS6381X_REG_WR(a, v) (0x8000 | ((a) << 8) | ((v) & 0xff))
#define MAKE_TPS6381X_REG_RD(a) (((a) << 8))
// Register R1 [0x1] -- CONTROL
enum slew_options {
    SLEW_1_0VDIVMS = 0,
    SLEW_2_5VDIVMS = 1,
    SLEW_5_0VDIVMS = 2,
    SLEW_10_0VDIVMS = 3,
};

enum control_fields_t {
    RANGE_OFF = 0x6,
    RANGE_MSK = 0x40,
    ENABLE_OFF = 0x5,
    ENABLE_MSK = 0x20,
    FPWM_OFF = 0x3,
    FPWM_MSK = 0x8,
    RPWM_OFF = 0x2,
    RPWM_MSK = 0x4,
    SLEW_OFF = 0x0,
    SLEW_MSK = 0x3,
};
#define GET_TPS6381X_RANGE(x) (((x) & RANGE_MSK) >> RANGE_OFF)
#define GET_TPS6381X_ENABLE(x) (((x) & ENABLE_MSK) >> ENABLE_OFF)
#define GET_TPS6381X_FPWM(x) (((x) & FPWM_MSK) >> FPWM_OFF)
#define GET_TPS6381X_RPWM(x) (((x) & RPWM_MSK) >> RPWM_OFF)
#define GET_TPS6381X_SLEW(x) (((x) & SLEW_MSK) >> SLEW_OFF)
#define SET_TPS6381X_RANGE(p, f) (p) = ((p) & ~RANGE_MSK) | (((f) << RANGE_OFF) & RANGE_MSK)
#define SET_TPS6381X_ENABLE(p, f) (p) = ((p) & ~ENABLE_MSK) | (((f) << ENABLE_OFF) & ENABLE_MSK)
#define SET_TPS6381X_FPWM(p, f) (p) = ((p) & ~FPWM_MSK) | (((f) << FPWM_OFF) & FPWM_MSK)
#define SET_TPS6381X_RPWM(p, f) (p) = ((p) & ~RPWM_MSK) | (((f) << RPWM_OFF) & RPWM_MSK)
#define SET_TPS6381X_SLEW(p, f) (p) = ((p) & ~SLEW_MSK) | (((f) << SLEW_OFF) & SLEW_MSK)

#define MAKE_TPS6381X_CONTROL(range, enable, fpwm, rpwm, slew) MAKE_TPS6381X_REG_WR(CONTROL, \
    (((range) << RANGE_OFF) & RANGE_MSK) |  \
    (((enable) << ENABLE_OFF) & ENABLE_MSK) |  \
    (((fpwm) << FPWM_OFF) & FPWM_MSK) |  \
    (((rpwm) << RPWM_OFF) & RPWM_MSK) |  \
    (((slew) << SLEW_OFF) & SLEW_MSK))
// Register R2 [0x2] -- STATUS

enum status_fields_t {
    TSD_OFF = 0x1,
    TSD_MSK = 0x2,
    PGN_OFF = 0x0,
    PGN_MSK = 0x1,
};
#define GET_TPS6381X_TSD(x) (((x) & TSD_MSK) >> TSD_OFF)
#define GET_TPS6381X_PGN(x) (((x) & PGN_MSK) >> PGN_OFF)
#define SET_TPS6381X_TSD(p, f) (p) = ((p) & ~TSD_MSK) | (((f) << TSD_OFF) & TSD_MSK)
#define SET_TPS6381X_PGN(p, f) (p) = ((p) & ~PGN_MSK) | (((f) << PGN_OFF) & PGN_MSK)

#define MAKE_TPS6381X_STATUS(tsd, pgn) MAKE_TPS6381X_REG_WR(STATUS, \
    (((tsd) << TSD_OFF) & TSD_MSK) |  \
    (((pgn) << PGN_OFF) & PGN_MSK))
// Register R3 [0x3] -- DEVID

// Register R4 [0x4] -- VOUT1

// Register R5 [0x5] -- VOUT2

