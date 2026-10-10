enum tca6424a_regs_t {
    IP0 = 0x0,
    IP1 = 0x1,
    IP2 = 0x2,
    OP0 = 0x4,
    OP1 = 0x5,
    OP2 = 0x6,
    PI0 = 0x8,
    PI1 = 0x9,
    PI2 = 0xa,
    CP0 = 0xc,
    CP1 = 0xd,
    CP2 = 0xe,
};
#define MAKE_TCA6424A_REG_WR(a, v) (((a) << 8) | ((v) & 0xff))
#define MAKE_TCA6424A_REG_RD(a) (0x8000 | ((a) << 8))
// Register R0 [0x0] -- IP0

// Register R1 [0x1] -- IP1

// Register R2 [0x2] -- IP2

// Register R4 [0x4] -- OP0

// Register R5 [0x5] -- OP1

// Register R6 [0x6] -- OP2

// Register R8 [0x8] -- PI0

// Register R9 [0x9] -- PI1

// Register R10 [0xa] -- PI2

// Register R12 [0xc] -- CP0

// Register R13 [0xd] -- CP1

// Register R14 [0xe] -- CP2

