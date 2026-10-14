enum si549_regs_t {
    DEVICE = 0x0,
    CFG = 0x7,
    ODC = 0x11,
    DIV_BY1 = 0x17,
    DIV_BY0 = 0x18,
    FBDIV_BY5 = 0x1a,
    FBDIV_BY4 = 0x1b,
    FBDIV_BY3 = 0x1c,
    FBDIV_BY2 = 0x1d,
    FBDIV_BY1 = 0x1e,
    FBDIV_BY0 = 0x1f,
    FCAL_OVR = 0x45,
    ADPLL_DELTA_M_BY2 = 0xe7,
    ADPLL_DELTA_M_BY1 = 0xe8,
    ADPLL_DELTA_M_BY0 = 0xe9,
    PAGE = 0xff,
};
#define MAKE_SI549_REG_WR(a, v) (0x800000 | ((a) << 8) | ((v) & 0xff))
#define MAKE_SI549_REG_RD(a) (((a) << 8))
// Register R0 [0x0] -- DEVICE

enum device_fields_t {
    TYPE_OFF = 0x0,
    TYPE_MSK = 0xff,
};
#define MAKE_SI549_DEVICE(type) MAKE_SI549_REG_WR(DEVICE, \
    (((type) << TYPE_OFF) & TYPE_MSK))
// Register R7 [0x7] -- CFG

enum cfg_fields_t {
    RESET_OFF = 0x7,
    RESET_MSK = 0x80,
    MS_ICAL2_OFF = 0x3,
    MS_ICAL2_MSK = 0x8,
};
#define MAKE_SI549_CFG(reset, ms_ical2) MAKE_SI549_REG_WR(CFG, \
    (((reset) << RESET_OFF) & RESET_MSK) |  \
    (((ms_ical2) << MS_ICAL2_OFF) & MS_ICAL2_MSK))
// Register R17 [0x11] -- ODC

enum odc_fields_t {
    OE_OFF = 0x0,
    OE_MSK = 0x1,
};
#define MAKE_SI549_ODC(oe) MAKE_SI549_REG_WR(ODC, \
    (((oe) << OE_OFF) & OE_MSK))
// Register R23 [0x17] -- DIV
enum ls_options {
    LS_DIV1 = 0,
    LS_DIV2 = 1,
    LS_DIV4 = 2,
    LS_DIV8 = 3,
    LS_DIV16 = 4,
    LS_DIV32 = 5,
    LS_DIV32A = 6,
    LS_DIV32B = 7,
};

enum div_fields_t {
    LS_OFF = 0xc,
    LS_MSK = 0x7000,
    HS_OFF = 0x0,
    HS_MSK = 0x7ff,
};
#define MAKE_SI549_DIV_LONG(ls, hs) ( \
    (((ls) << LS_OFF) & LS_MSK) |  \
    (((hs) << HS_OFF) & HS_MSK))
#define MAKE_SI549_DIV_BY0(value) MAKE_SI549_REG_WR(DIV_BY0, (((value) << 0) & 0xff))
#define MAKE_SI549_DIV_BY1(value) MAKE_SI549_REG_WR(DIV_BY1, (((value) >> 8) & 0x77))
// Register R26 [0x1a] -- FBDIV


#define MAKE_SI549_FBDIV_BY0(value) MAKE_SI549_REG_WR(FBDIV_BY0, (((value) << 0) & 0xff))
#define MAKE_SI549_FBDIV_BY1(value) MAKE_SI549_REG_WR(FBDIV_BY1, (((value) >> 8) & 0xff))
#define MAKE_SI549_FBDIV_BY2(value) MAKE_SI549_REG_WR(FBDIV_BY2, (((value) >> 16) & 0xff))
#define MAKE_SI549_FBDIV_BY3(value) MAKE_SI549_REG_WR(FBDIV_BY3, (((value) >> 24) & 0xff))
#define MAKE_SI549_FBDIV_BY4(value) MAKE_SI549_REG_WR(FBDIV_BY4, (((value) >> 32) & 0xff))
#define MAKE_SI549_FBDIV_BY5(value) MAKE_SI549_REG_WR(FBDIV_BY5, (((value) >> 40) & 0xff))
// Register R69 [0x45] -- FCAL_OVR

enum fcal_ovr_fields_t {
    FCAL_OVR_OFF = 0x7,
    FCAL_OVR_MSK = 0x80,
    RESERVED_OFF = 0x1,
    RESERVED_MSK = 0x2,
};
#define MAKE_SI549_FCAL_OVR(fcal_ovr, reserved) MAKE_SI549_REG_WR(FCAL_OVR, \
    (((fcal_ovr) << FCAL_OVR_OFF) & FCAL_OVR_MSK) |  \
    (((reserved) << RESERVED_OFF) & RESERVED_MSK))
// Register R231 [0xe7] -- ADPLL_DELTA_M


#define MAKE_SI549_ADPLL_DELTA_M_BY0(value) MAKE_SI549_REG_WR(ADPLL_DELTA_M_BY0, (((value) << 0) & 0xff))
#define MAKE_SI549_ADPLL_DELTA_M_BY1(value) MAKE_SI549_REG_WR(ADPLL_DELTA_M_BY1, (((value) >> 8) & 0xff))
#define MAKE_SI549_ADPLL_DELTA_M_BY2(value) MAKE_SI549_REG_WR(ADPLL_DELTA_M_BY2, (((value) >> 16) & 0xff))
// Register R255 [0xff] -- PAGE

