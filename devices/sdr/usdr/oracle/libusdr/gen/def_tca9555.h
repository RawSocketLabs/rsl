enum tca9555_regs_t {
    GSR1 = 0x0,
    GSR2 = 0x1,
    OCR1 = 0x2,
    OCR2 = 0x3,
    PIR1 = 0x4,
    PIR2 = 0x5,
    GCR1 = 0x6,
    GCR2 = 0x7,
    PUR1 = 0x8,
    PUR2 = 0x9,
    IER1 = 0xa,
    IER2 = 0xb,
    TSCR1 = 0xc,
    TSCR2 = 0xd,
    ISR1 = 0xe,
    ISR2 = 0xf,
    REIR1 = 0x10,
    REIR2 = 0x11,
    FEIR1 = 0x12,
    FEIR2 = 0x13,
    IFR1 = 0x14,
    IFR2 = 0x15,
};
#define MAKE_TCA9555_REG_WR(a, v) (((a) << 8) | ((v) & 0xff))
#define MAKE_TCA9555_REG_RD(a) (0x8000 | ((a) << 8))
// Register R0 [0x0] -- GSR1

// Register R1 [0x1] -- GSR2

// Register R2 [0x2] -- OCR1

// Register R3 [0x3] -- OCR2

// Register R4 [0x4] -- PIR1

// Register R5 [0x5] -- PIR2

// Register R6 [0x6] -- GCR1

// Register R7 [0x7] -- GCR2

// Register R8 [0x8] -- PUR1

// Register R9 [0x9] -- PUR2

// Register R10 [0xa] -- IER1

// Register R11 [0xb] -- IER2

// Register R12 [0xc] -- TSCR1

// Register R13 [0xd] -- TSCR2

// Register R14 [0xe] -- ISR1

// Register R15 [0xf] -- ISR2

// Register R16 [0x10] -- REIR1

// Register R17 [0x11] -- REIR2

// Register R18 [0x12] -- FEIR1

// Register R19 [0x13] -- FEIR2

// Register R20 [0x14] -- IFR1

// Register R21 [0x15] -- IFR2

