enum xra1405_regs_t {
    GSR1 = 0x0,
    GSR2 = 0x2,
    OCR1 = 0x4,
    OCR2 = 0x6,
    PIR1 = 0x8,
    PIR2 = 0xa,
    GCR1 = 0xc,
    GCR2 = 0xe,
    PUR1 = 0x10,
    PUR2 = 0x12,
    IER1 = 0x14,
    IER2 = 0x16,
    TSCR1 = 0x18,
    TSCR2 = 0x1a,
    ISR1 = 0x1c,
    ISR2 = 0x1d,
    REIR1 = 0x20,
    REIR2 = 0x22,
    FEIR1 = 0x24,
    FEIR2 = 0x26,
    IFR1 = 0x28,
    IFR2 = 0x2a,
};
#define MAKE_XRA1405_REG_WR(a, v) (((a) << 8) | ((v) & 0xff))
#define MAKE_XRA1405_REG_RD(a) (0x8000 | ((a) << 8))
// Register R0 [0x0] -- GSR1

// Register R2 [0x2] -- GSR2

// Register R4 [0x4] -- OCR1

// Register R6 [0x6] -- OCR2

// Register R8 [0x8] -- PIR1

// Register R10 [0xa] -- PIR2

// Register R12 [0xc] -- GCR1

// Register R14 [0xe] -- GCR2

// Register R16 [0x10] -- PUR1

// Register R18 [0x12] -- PUR2

// Register R20 [0x14] -- IER1

// Register R22 [0x16] -- IER2

// Register R24 [0x18] -- TSCR1

// Register R26 [0x1a] -- TSCR2

// Register R28 [0x1c] -- ISR1

// Register R29 [0x1d] -- ISR2

// Register R32 [0x20] -- REIR1

// Register R34 [0x22] -- REIR2

// Register R36 [0x24] -- FEIR1

// Register R38 [0x26] -- FEIR2

// Register R40 [0x28] -- IFR1

// Register R42 [0x2a] -- IFR2

