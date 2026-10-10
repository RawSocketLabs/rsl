enum lmk04832_regs_t {
    RESET = 0x0,
    LMKDEV_TYPE = 0x3,
    PROD_HI = 0x4,
    PROD_LOW = 0x5,
    MASKREV = 0x6,
    VNDR_HI = 0xc,
    VNDR_LOW = 0xd,
    DCLK0_1_DIV_LOW = 0x100,
    DCLK0_1_DDLY_LOW = 0x101,
    DCLK0_1_CFG = 0x102,
    DELAY_CTRL = 0x103,
    SCLK0_1_CFG = 0x104,
    SCLK0_1_ADLY = 0x105,
    SCLKX_Y_DDLY = 0x106,
    CLKOUT0_1_FMT = 0x107,
    VCO_BUF_CFG = 0x138,
    FB_CFG = 0x13f,
    OSC_SYSREF_CFG = 0x140,
    PLL1_2_SYNC = 0x145,
    CLKIN_CFG = 0x146,
    CLKIN_TYPE = 0x147,
    RESET_CFG = 0x14a,
    CLKIN0_R_HI = 0x153,
    CLKIN0_R_LOW = 0x154,
    CLKIN1_R_HI = 0x155,
    CLKIN1_R_LOW = 0x156,
    CLKIN2_R_HI = 0x157,
    CLKIN2_R_LOW = 0x158,
    PLL1_N_HI = 0x159,
    PLL1_N_LOW = 0x15a,
    PLL1_DLD_CNT_HI = 0x15c,
    PLL1_DLD_CNT_LOW = 0x15d,
    PLL2_R_HI = 0x160,
    PLL2_R_LOW = 0x161,
    PLL2_FUNC = 0x162,
    PLL2_N_CAL_HI = 0x163,
    PLL2_N_CAL_MID = 0x164,
    PLL2_N_CAL_LOW = 0x165,
    PLL2_N_HI = 0x166,
    PLL2_N_MID = 0x167,
    PLL2_N_LOW = 0x168,
    PLL2_PD = 0x173,
};
#define MAKE_LMK04832_REG_WR(a, v) (((a) << 8) | ((v) & 0xff))
#define MAKE_LMK04832_REG_RD(a) (0x80000000 | ((a) << 8))
// Register R0 [0x0] -- RESET

enum reset_fields_t {
    RESET_OFF = 0x7,
    RESET_MSK = 0x80,
    SPI_3WARE_DIS_OFF = 0x4,
    SPI_3WARE_DIS_MSK = 0x10,
};
#define GET_LMK04832_RESET(x) (((x) & RESET_MSK) >> RESET_OFF)
#define GET_LMK04832_SPI_3WARE_DIS(x) (((x) & SPI_3WARE_DIS_MSK) >> SPI_3WARE_DIS_OFF)
#define SET_LMK04832_RESET(p, f) (p) = ((p) & ~RESET_MSK) | (((f) << RESET_OFF) & RESET_MSK)
#define SET_LMK04832_SPI_3WARE_DIS(p, f) (p) = ((p) & ~SPI_3WARE_DIS_MSK) | (((f) << SPI_3WARE_DIS_OFF) & SPI_3WARE_DIS_MSK)

#define MAKE_LMK04832_RESET(reset, spi_3ware_dis) MAKE_LMK04832_REG_WR(RESET, \
    (((reset) << RESET_OFF) & RESET_MSK) |  \
    (((spi_3ware_dis) << SPI_3WARE_DIS_OFF) & SPI_3WARE_DIS_MSK))
// Register R3 [0x3] -- LMKDEV_TYPE

enum lmkdev_type_fields_t {
    LMKDEV_TYPE_OFF = 0x0,
    LMKDEV_TYPE_MSK = 0xff,
};
#define GET_LMK04832_LMKDEV_TYPE(x) (((x) & LMKDEV_TYPE_MSK) >> LMKDEV_TYPE_OFF)
#define SET_LMK04832_LMKDEV_TYPE(p, f) (p) = ((p) & ~LMKDEV_TYPE_MSK) | (((f) << LMKDEV_TYPE_OFF) & LMKDEV_TYPE_MSK)

#define MAKE_LMK04832_LMKDEV_TYPE(lmkdev_type) MAKE_LMK04832_REG_WR(LMKDEV_TYPE, \
    (((lmkdev_type) << LMKDEV_TYPE_OFF) & LMKDEV_TYPE_MSK))
// Register R4 [0x4] -- PROD_HI

enum prod_hi_fields_t {
    PROD_HI_OFF = 0x0,
    PROD_HI_MSK = 0xff,
};
#define GET_LMK04832_PROD_HI(x) (((x) & PROD_HI_MSK) >> PROD_HI_OFF)
#define SET_LMK04832_PROD_HI(p, f) (p) = ((p) & ~PROD_HI_MSK) | (((f) << PROD_HI_OFF) & PROD_HI_MSK)

#define MAKE_LMK04832_PROD_HI(prod_hi) MAKE_LMK04832_REG_WR(PROD_HI, \
    (((prod_hi) << PROD_HI_OFF) & PROD_HI_MSK))
// Register R5 [0x5] -- PROD_LOW

enum prod_low_fields_t {
    PROD_LOW_OFF = 0x0,
    PROD_LOW_MSK = 0xff,
};
#define GET_LMK04832_PROD_LOW(x) (((x) & PROD_LOW_MSK) >> PROD_LOW_OFF)
#define SET_LMK04832_PROD_LOW(p, f) (p) = ((p) & ~PROD_LOW_MSK) | (((f) << PROD_LOW_OFF) & PROD_LOW_MSK)

#define MAKE_LMK04832_PROD_LOW(prod_low) MAKE_LMK04832_REG_WR(PROD_LOW, \
    (((prod_low) << PROD_LOW_OFF) & PROD_LOW_MSK))
// Register R6 [0x6] -- MASKREV

enum maskrev_fields_t {
    MASKREV_OFF = 0x0,
    MASKREV_MSK = 0xff,
};
#define GET_LMK04832_MASKREV(x) (((x) & MASKREV_MSK) >> MASKREV_OFF)
#define SET_LMK04832_MASKREV(p, f) (p) = ((p) & ~MASKREV_MSK) | (((f) << MASKREV_OFF) & MASKREV_MSK)

#define MAKE_LMK04832_MASKREV(maskrev) MAKE_LMK04832_REG_WR(MASKREV, \
    (((maskrev) << MASKREV_OFF) & MASKREV_MSK))
// Register R12 [0xc] -- VNDR_HI

enum vndr_hi_fields_t {
    VNDR_HI_OFF = 0x0,
    VNDR_HI_MSK = 0xff,
};
#define GET_LMK04832_VNDR_HI(x) (((x) & VNDR_HI_MSK) >> VNDR_HI_OFF)
#define SET_LMK04832_VNDR_HI(p, f) (p) = ((p) & ~VNDR_HI_MSK) | (((f) << VNDR_HI_OFF) & VNDR_HI_MSK)

#define MAKE_LMK04832_VNDR_HI(vndr_hi) MAKE_LMK04832_REG_WR(VNDR_HI, \
    (((vndr_hi) << VNDR_HI_OFF) & VNDR_HI_MSK))
// Register R13 [0xd] -- VNDR_LOW

enum vndr_low_fields_t {
    VNDR_LOW_OFF = 0x0,
    VNDR_LOW_MSK = 0xff,
};
#define GET_LMK04832_VNDR_LOW(x) (((x) & VNDR_LOW_MSK) >> VNDR_LOW_OFF)
#define SET_LMK04832_VNDR_LOW(p, f) (p) = ((p) & ~VNDR_LOW_MSK) | (((f) << VNDR_LOW_OFF) & VNDR_LOW_MSK)

#define MAKE_LMK04832_VNDR_LOW(vndr_low) MAKE_LMK04832_REG_WR(VNDR_LOW, \
    (((vndr_low) << VNDR_LOW_OFF) & VNDR_LOW_MSK))
// Register R256 [0x100] -- DCLK0_1_DIV_LOW

enum dclk0_1_div_low_fields_t {
    DCLK0_1_DIV_LOW_OFF = 0x0,
    DCLK0_1_DIV_LOW_MSK = 0xff,
};
#define GET_LMK04832_DCLK0_1_DIV_LOW(x) (((x) & DCLK0_1_DIV_LOW_MSK) >> DCLK0_1_DIV_LOW_OFF)
#define SET_LMK04832_DCLK0_1_DIV_LOW(p, f) (p) = ((p) & ~DCLK0_1_DIV_LOW_MSK) | (((f) << DCLK0_1_DIV_LOW_OFF) & DCLK0_1_DIV_LOW_MSK)

#define MAKE_LMK04832_DCLK0_1_DIV_LOW(dclk0_1_div_low) MAKE_LMK04832_REG_WR(DCLK0_1_DIV_LOW, \
    (((dclk0_1_div_low) << DCLK0_1_DIV_LOW_OFF) & DCLK0_1_DIV_LOW_MSK))
// Register R257 [0x101] -- DCLK0_1_DDLY_LOW

enum dclk0_1_ddly_low_fields_t {
    DCLK0_1_DDLY_LOW_OFF = 0x0,
    DCLK0_1_DDLY_LOW_MSK = 0xff,
};
#define GET_LMK04832_DCLK0_1_DDLY_LOW(x) (((x) & DCLK0_1_DDLY_LOW_MSK) >> DCLK0_1_DDLY_LOW_OFF)
#define SET_LMK04832_DCLK0_1_DDLY_LOW(p, f) (p) = ((p) & ~DCLK0_1_DDLY_LOW_MSK) | (((f) << DCLK0_1_DDLY_LOW_OFF) & DCLK0_1_DDLY_LOW_MSK)

#define MAKE_LMK04832_DCLK0_1_DDLY_LOW(dclk0_1_ddly_low) MAKE_LMK04832_REG_WR(DCLK0_1_DDLY_LOW, \
    (((dclk0_1_ddly_low) << DCLK0_1_DDLY_LOW_OFF) & DCLK0_1_DDLY_LOW_MSK))
// Register R258 [0x102] -- DCLK0_1_CFG

enum dclk0_1_cfg_fields_t {
    CLKOUT0_1_PD_OFF = 0x7,
    CLKOUT0_1_PD_MSK = 0x80,
    CLKOUT0_1_ODL_OFF = 0x6,
    CLKOUT0_1_ODL_MSK = 0x40,
    CLKOUT0_1_IDL_OFF = 0x5,
    CLKOUT0_1_IDL_MSK = 0x20,
    CLKOUT0_1_DDLY_PD_OFF = 0x4,
    CLKOUT0_1_DDLY_PD_MSK = 0x10,
    DCLK0_1_DDLY_HI_OFF = 0x2,
    DCLK0_1_DDLY_HI_MSK = 0xc,
    DCLK0_1_DIV_HI_OFF = 0x0,
    DCLK0_1_DIV_HI_MSK = 0x3,
};
#define GET_LMK04832_CLKOUT0_1_PD(x) (((x) & CLKOUT0_1_PD_MSK) >> CLKOUT0_1_PD_OFF)
#define GET_LMK04832_CLKOUT0_1_ODL(x) (((x) & CLKOUT0_1_ODL_MSK) >> CLKOUT0_1_ODL_OFF)
#define GET_LMK04832_CLKOUT0_1_IDL(x) (((x) & CLKOUT0_1_IDL_MSK) >> CLKOUT0_1_IDL_OFF)
#define GET_LMK04832_CLKOUT0_1_DDLY_PD(x) (((x) & CLKOUT0_1_DDLY_PD_MSK) >> CLKOUT0_1_DDLY_PD_OFF)
#define GET_LMK04832_DCLK0_1_DDLY_HI(x) (((x) & DCLK0_1_DDLY_HI_MSK) >> DCLK0_1_DDLY_HI_OFF)
#define GET_LMK04832_DCLK0_1_DIV_HI(x) (((x) & DCLK0_1_DIV_HI_MSK) >> DCLK0_1_DIV_HI_OFF)
#define SET_LMK04832_CLKOUT0_1_PD(p, f) (p) = ((p) & ~CLKOUT0_1_PD_MSK) | (((f) << CLKOUT0_1_PD_OFF) & CLKOUT0_1_PD_MSK)
#define SET_LMK04832_CLKOUT0_1_ODL(p, f) (p) = ((p) & ~CLKOUT0_1_ODL_MSK) | (((f) << CLKOUT0_1_ODL_OFF) & CLKOUT0_1_ODL_MSK)
#define SET_LMK04832_CLKOUT0_1_IDL(p, f) (p) = ((p) & ~CLKOUT0_1_IDL_MSK) | (((f) << CLKOUT0_1_IDL_OFF) & CLKOUT0_1_IDL_MSK)
#define SET_LMK04832_CLKOUT0_1_DDLY_PD(p, f) (p) = ((p) & ~CLKOUT0_1_DDLY_PD_MSK) | (((f) << CLKOUT0_1_DDLY_PD_OFF) & CLKOUT0_1_DDLY_PD_MSK)
#define SET_LMK04832_DCLK0_1_DDLY_HI(p, f) (p) = ((p) & ~DCLK0_1_DDLY_HI_MSK) | (((f) << DCLK0_1_DDLY_HI_OFF) & DCLK0_1_DDLY_HI_MSK)
#define SET_LMK04832_DCLK0_1_DIV_HI(p, f) (p) = ((p) & ~DCLK0_1_DIV_HI_MSK) | (((f) << DCLK0_1_DIV_HI_OFF) & DCLK0_1_DIV_HI_MSK)

#define MAKE_LMK04832_DCLK0_1_CFG(clkout0_1_pd, clkout0_1_odl, clkout0_1_idl, clkout0_1_ddly_pd, dclk0_1_ddly_hi, dclk0_1_div_hi) MAKE_LMK04832_REG_WR(DCLK0_1_CFG, \
    (((clkout0_1_pd) << CLKOUT0_1_PD_OFF) & CLKOUT0_1_PD_MSK) |  \
    (((clkout0_1_odl) << CLKOUT0_1_ODL_OFF) & CLKOUT0_1_ODL_MSK) |  \
    (((clkout0_1_idl) << CLKOUT0_1_IDL_OFF) & CLKOUT0_1_IDL_MSK) |  \
    (((clkout0_1_ddly_pd) << CLKOUT0_1_DDLY_PD_OFF) & CLKOUT0_1_DDLY_PD_MSK) |  \
    (((dclk0_1_ddly_hi) << DCLK0_1_DDLY_HI_OFF) & DCLK0_1_DDLY_HI_MSK) |  \
    (((dclk0_1_div_hi) << DCLK0_1_DIV_HI_OFF) & DCLK0_1_DIV_HI_MSK))
// Register R259 [0x103] -- DELAY_CTRL
enum clkout0_src_mux_options {
    CLKOUT0_SRC_MUX_DEVICE_CLOCK = 0,
    CLKOUT0_SRC_MUX_SYSREF = 1,
};

enum delay_ctrl_fields_t {
    CLKOUT0_SRC_MUX_OFF = 0x5,
    CLKOUT0_SRC_MUX_MSK = 0x20,
    DCLK0_1_PD_OFF = 0x4,
    DCLK0_1_PD_MSK = 0x10,
    DCLK0_BYP_OFF = 0x3,
    DCLK0_BYP_MSK = 0x8,
    DCLK0_1_DCC_OFF = 0x2,
    DCLK0_1_DCC_MSK = 0x4,
    DCLK0_1_POL_OFF = 0x1,
    DCLK0_1_POL_MSK = 0x2,
    DCLK0_1_HS_OFF = 0x0,
    DCLK0_1_HS_MSK = 0x1,
};
#define GET_LMK04832_CLKOUT0_SRC_MUX(x) (((x) & CLKOUT0_SRC_MUX_MSK) >> CLKOUT0_SRC_MUX_OFF)
#define GET_LMK04832_DCLK0_1_PD(x) (((x) & DCLK0_1_PD_MSK) >> DCLK0_1_PD_OFF)
#define GET_LMK04832_DCLK0_BYP(x) (((x) & DCLK0_BYP_MSK) >> DCLK0_BYP_OFF)
#define GET_LMK04832_DCLK0_1_DCC(x) (((x) & DCLK0_1_DCC_MSK) >> DCLK0_1_DCC_OFF)
#define GET_LMK04832_DCLK0_1_POL(x) (((x) & DCLK0_1_POL_MSK) >> DCLK0_1_POL_OFF)
#define GET_LMK04832_DCLK0_1_HS(x) (((x) & DCLK0_1_HS_MSK) >> DCLK0_1_HS_OFF)
#define SET_LMK04832_CLKOUT0_SRC_MUX(p, f) (p) = ((p) & ~CLKOUT0_SRC_MUX_MSK) | (((f) << CLKOUT0_SRC_MUX_OFF) & CLKOUT0_SRC_MUX_MSK)
#define SET_LMK04832_DCLK0_1_PD(p, f) (p) = ((p) & ~DCLK0_1_PD_MSK) | (((f) << DCLK0_1_PD_OFF) & DCLK0_1_PD_MSK)
#define SET_LMK04832_DCLK0_BYP(p, f) (p) = ((p) & ~DCLK0_BYP_MSK) | (((f) << DCLK0_BYP_OFF) & DCLK0_BYP_MSK)
#define SET_LMK04832_DCLK0_1_DCC(p, f) (p) = ((p) & ~DCLK0_1_DCC_MSK) | (((f) << DCLK0_1_DCC_OFF) & DCLK0_1_DCC_MSK)
#define SET_LMK04832_DCLK0_1_POL(p, f) (p) = ((p) & ~DCLK0_1_POL_MSK) | (((f) << DCLK0_1_POL_OFF) & DCLK0_1_POL_MSK)
#define SET_LMK04832_DCLK0_1_HS(p, f) (p) = ((p) & ~DCLK0_1_HS_MSK) | (((f) << DCLK0_1_HS_OFF) & DCLK0_1_HS_MSK)

#define MAKE_LMK04832_DELAY_CTRL(clkout0_src_mux, dclk0_1_pd, dclk0_byp, dclk0_1_dcc, dclk0_1_pol, dclk0_1_hs) MAKE_LMK04832_REG_WR(DELAY_CTRL, \
    (((clkout0_src_mux) << CLKOUT0_SRC_MUX_OFF) & CLKOUT0_SRC_MUX_MSK) |  \
    (((dclk0_1_pd) << DCLK0_1_PD_OFF) & DCLK0_1_PD_MSK) |  \
    (((dclk0_byp) << DCLK0_BYP_OFF) & DCLK0_BYP_MSK) |  \
    (((dclk0_1_dcc) << DCLK0_1_DCC_OFF) & DCLK0_1_DCC_MSK) |  \
    (((dclk0_1_pol) << DCLK0_1_POL_OFF) & DCLK0_1_POL_MSK) |  \
    (((dclk0_1_hs) << DCLK0_1_HS_OFF) & DCLK0_1_HS_MSK))
// Register R260 [0x104] -- SCLK0_1_CFG
enum clkout1_src_mux_options {
    CLKOUT1_SRC_MUX_DEVICE_CLOCK = 0,
    CLKOUT1_SRC_MUX_SYSREF = 1,
};

enum sclk0_1_cfg_fields_t {
    CLKOUT1_SRC_MUX_OFF = 0x5,
    CLKOUT1_SRC_MUX_MSK = 0x20,
    SCLK0_1_PD_OFF = 0x4,
    SCLK0_1_PD_MSK = 0x10,
    SCLK0_1_DIS_MODE_OFF = 0x2,
    SCLK0_1_DIS_MODE_MSK = 0xc,
    SCLK0_1_POL_OFF = 0x1,
    SCLK0_1_POL_MSK = 0x2,
    SCLK0_1_HS_OFF = 0x0,
    SCLK0_1_HS_MSK = 0x1,
};
#define GET_LMK04832_CLKOUT1_SRC_MUX(x) (((x) & CLKOUT1_SRC_MUX_MSK) >> CLKOUT1_SRC_MUX_OFF)
#define GET_LMK04832_SCLK0_1_PD(x) (((x) & SCLK0_1_PD_MSK) >> SCLK0_1_PD_OFF)
#define GET_LMK04832_SCLK0_1_DIS_MODE(x) (((x) & SCLK0_1_DIS_MODE_MSK) >> SCLK0_1_DIS_MODE_OFF)
#define GET_LMK04832_SCLK0_1_POL(x) (((x) & SCLK0_1_POL_MSK) >> SCLK0_1_POL_OFF)
#define GET_LMK04832_SCLK0_1_HS(x) (((x) & SCLK0_1_HS_MSK) >> SCLK0_1_HS_OFF)
#define SET_LMK04832_CLKOUT1_SRC_MUX(p, f) (p) = ((p) & ~CLKOUT1_SRC_MUX_MSK) | (((f) << CLKOUT1_SRC_MUX_OFF) & CLKOUT1_SRC_MUX_MSK)
#define SET_LMK04832_SCLK0_1_PD(p, f) (p) = ((p) & ~SCLK0_1_PD_MSK) | (((f) << SCLK0_1_PD_OFF) & SCLK0_1_PD_MSK)
#define SET_LMK04832_SCLK0_1_DIS_MODE(p, f) (p) = ((p) & ~SCLK0_1_DIS_MODE_MSK) | (((f) << SCLK0_1_DIS_MODE_OFF) & SCLK0_1_DIS_MODE_MSK)
#define SET_LMK04832_SCLK0_1_POL(p, f) (p) = ((p) & ~SCLK0_1_POL_MSK) | (((f) << SCLK0_1_POL_OFF) & SCLK0_1_POL_MSK)
#define SET_LMK04832_SCLK0_1_HS(p, f) (p) = ((p) & ~SCLK0_1_HS_MSK) | (((f) << SCLK0_1_HS_OFF) & SCLK0_1_HS_MSK)

#define MAKE_LMK04832_SCLK0_1_CFG(clkout1_src_mux, sclk0_1_pd, sclk0_1_dis_mode, sclk0_1_pol, sclk0_1_hs) MAKE_LMK04832_REG_WR(SCLK0_1_CFG, \
    (((clkout1_src_mux) << CLKOUT1_SRC_MUX_OFF) & CLKOUT1_SRC_MUX_MSK) |  \
    (((sclk0_1_pd) << SCLK0_1_PD_OFF) & SCLK0_1_PD_MSK) |  \
    (((sclk0_1_dis_mode) << SCLK0_1_DIS_MODE_OFF) & SCLK0_1_DIS_MODE_MSK) |  \
    (((sclk0_1_pol) << SCLK0_1_POL_OFF) & SCLK0_1_POL_MSK) |  \
    (((sclk0_1_hs) << SCLK0_1_HS_OFF) & SCLK0_1_HS_MSK))
// Register R261 [0x105] -- SCLK0_1_ADLY

enum sclk0_1_adly_fields_t {
    SCLK0_1_ADLY_EN_OFF = 0x5,
    SCLK0_1_ADLY_EN_MSK = 0x20,
    SCLK0_1_ADLY_OFF = 0x0,
    SCLK0_1_ADLY_MSK = 0x1f,
};
#define GET_LMK04832_SCLK0_1_ADLY_EN(x) (((x) & SCLK0_1_ADLY_EN_MSK) >> SCLK0_1_ADLY_EN_OFF)
#define GET_LMK04832_SCLK0_1_ADLY(x) (((x) & SCLK0_1_ADLY_MSK) >> SCLK0_1_ADLY_OFF)
#define SET_LMK04832_SCLK0_1_ADLY_EN(p, f) (p) = ((p) & ~SCLK0_1_ADLY_EN_MSK) | (((f) << SCLK0_1_ADLY_EN_OFF) & SCLK0_1_ADLY_EN_MSK)
#define SET_LMK04832_SCLK0_1_ADLY(p, f) (p) = ((p) & ~SCLK0_1_ADLY_MSK) | (((f) << SCLK0_1_ADLY_OFF) & SCLK0_1_ADLY_MSK)

#define MAKE_LMK04832_SCLK0_1_ADLY(sclk0_1_adly_en, sclk0_1_adly) MAKE_LMK04832_REG_WR(SCLK0_1_ADLY, \
    (((sclk0_1_adly_en) << SCLK0_1_ADLY_EN_OFF) & SCLK0_1_ADLY_EN_MSK) |  \
    (((sclk0_1_adly) << SCLK0_1_ADLY_OFF) & SCLK0_1_ADLY_MSK))
// Register R262 [0x106] -- SCLKX_Y_DDLY

enum sclkx_y_ddly_fields_t {
    SCLKX_Y_DDLY_OFF = 0x0,
    SCLKX_Y_DDLY_MSK = 0xf,
};
#define GET_LMK04832_SCLKX_Y_DDLY(x) (((x) & SCLKX_Y_DDLY_MSK) >> SCLKX_Y_DDLY_OFF)
#define SET_LMK04832_SCLKX_Y_DDLY(p, f) (p) = ((p) & ~SCLKX_Y_DDLY_MSK) | (((f) << SCLKX_Y_DDLY_OFF) & SCLKX_Y_DDLY_MSK)

#define MAKE_LMK04832_SCLKX_Y_DDLY(sclkx_y_ddly) MAKE_LMK04832_REG_WR(SCLKX_Y_DDLY, \
    (((sclkx_y_ddly) << SCLKX_Y_DDLY_OFF) & SCLKX_Y_DDLY_MSK))
// Register R263 [0x107] -- CLKOUT0_1_FMT

enum clkout0_1_fmt_fields_t {
    CLKOUT1_FMT_OFF = 0x4,
    CLKOUT1_FMT_MSK = 0xf0,
    CLKOUT0_FMT_OFF = 0x0,
    CLKOUT0_FMT_MSK = 0xf,
};
#define GET_LMK04832_CLKOUT1_FMT(x) (((x) & CLKOUT1_FMT_MSK) >> CLKOUT1_FMT_OFF)
#define GET_LMK04832_CLKOUT0_FMT(x) (((x) & CLKOUT0_FMT_MSK) >> CLKOUT0_FMT_OFF)
#define SET_LMK04832_CLKOUT1_FMT(p, f) (p) = ((p) & ~CLKOUT1_FMT_MSK) | (((f) << CLKOUT1_FMT_OFF) & CLKOUT1_FMT_MSK)
#define SET_LMK04832_CLKOUT0_FMT(p, f) (p) = ((p) & ~CLKOUT0_FMT_MSK) | (((f) << CLKOUT0_FMT_OFF) & CLKOUT0_FMT_MSK)

#define MAKE_LMK04832_CLKOUT0_1_FMT(clkout1_fmt, clkout0_fmt) MAKE_LMK04832_REG_WR(CLKOUT0_1_FMT, \
    (((clkout1_fmt) << CLKOUT1_FMT_OFF) & CLKOUT1_FMT_MSK) |  \
    (((clkout0_fmt) << CLKOUT0_FMT_OFF) & CLKOUT0_FMT_MSK))
// Register R312 [0x138] -- VCO_BUF_CFG
enum vco_mux_options {
    VCO_MUX_VCO0 = 0,
    VCO_MUX_VCO1 = 1,
    VCO_MUX_CLKIN1 = 2,
};
enum oscout_mux_options {
    OSCOUT_MUX_BUFF_OSCIN = 0,
    OSCOUT_MUX_FEEDBACK_MUX = 1,
};
enum oscout_fmt_options {
    OSCOUT_FMT_POWER_DOWN = 0,
};

enum vco_buf_cfg_fields_t {
    VCO_MUX_OFF = 0x5,
    VCO_MUX_MSK = 0x60,
    OSCOUT_MUX_OFF = 0x4,
    OSCOUT_MUX_MSK = 0x10,
    OSCOUT_FMT_OFF = 0x0,
    OSCOUT_FMT_MSK = 0xf,
};
#define GET_LMK04832_VCO_MUX(x) (((x) & VCO_MUX_MSK) >> VCO_MUX_OFF)
#define GET_LMK04832_OSCOUT_MUX(x) (((x) & OSCOUT_MUX_MSK) >> OSCOUT_MUX_OFF)
#define GET_LMK04832_OSCOUT_FMT(x) (((x) & OSCOUT_FMT_MSK) >> OSCOUT_FMT_OFF)
#define SET_LMK04832_VCO_MUX(p, f) (p) = ((p) & ~VCO_MUX_MSK) | (((f) << VCO_MUX_OFF) & VCO_MUX_MSK)
#define SET_LMK04832_OSCOUT_MUX(p, f) (p) = ((p) & ~OSCOUT_MUX_MSK) | (((f) << OSCOUT_MUX_OFF) & OSCOUT_MUX_MSK)
#define SET_LMK04832_OSCOUT_FMT(p, f) (p) = ((p) & ~OSCOUT_FMT_MSK) | (((f) << OSCOUT_FMT_OFF) & OSCOUT_FMT_MSK)

#define MAKE_LMK04832_VCO_BUF_CFG(vco_mux, oscout_mux, oscout_fmt) MAKE_LMK04832_REG_WR(VCO_BUF_CFG, \
    (((vco_mux) << VCO_MUX_OFF) & VCO_MUX_MSK) |  \
    (((oscout_mux) << OSCOUT_MUX_OFF) & OSCOUT_MUX_MSK) |  \
    (((oscout_fmt) << OSCOUT_FMT_OFF) & OSCOUT_FMT_MSK))
// Register R319 [0x13f] -- FB_CFG
enum pll2_rclk_mux_options {
    PLL2_RCLK_MUX_OSCIN = 0,
    PLL2_RCLK_MUX_CLKIN = 1,
};
enum pll2_nclk_mux_options {
    PLL2_NCLK_MUX_PLL2_PRESCALER = 0,
    PLL2_NCLK_MUX_FEEDBACK_MUX = 1,
};
enum pll1_nclk_mux_options {
    PLL1_NCLK_MUX_FEEDBACK_MUX = 0,
    PLL1_NCLK_MUX_OSCIN = 1,
    PLL1_NCLK_MUX_PLL2_PRESCALER = 2,
};
enum fb_mux_options {
    FB_MUX_CLKOUT6 = 0,
    FB_MUX_CLKOUT8 = 1,
    FB_MUX_SYSREF_DIVIDER = 2,
    FB_MUX_EXTERNAL = 3,
};

enum fb_cfg_fields_t {
    PLL2_RCLK_MUX_OFF = 0x7,
    PLL2_RCLK_MUX_MSK = 0x80,
    PLL2_NCLK_MUX_OFF = 0x5,
    PLL2_NCLK_MUX_MSK = 0x20,
    PLL1_NCLK_MUX_OFF = 0x3,
    PLL1_NCLK_MUX_MSK = 0x18,
    FB_MUX_OFF = 0x1,
    FB_MUX_MSK = 0x6,
    FB_MUX_EN_OFF = 0x0,
    FB_MUX_EN_MSK = 0x1,
};
#define GET_LMK04832_PLL2_RCLK_MUX(x) (((x) & PLL2_RCLK_MUX_MSK) >> PLL2_RCLK_MUX_OFF)
#define GET_LMK04832_PLL2_NCLK_MUX(x) (((x) & PLL2_NCLK_MUX_MSK) >> PLL2_NCLK_MUX_OFF)
#define GET_LMK04832_PLL1_NCLK_MUX(x) (((x) & PLL1_NCLK_MUX_MSK) >> PLL1_NCLK_MUX_OFF)
#define GET_LMK04832_FB_MUX(x) (((x) & FB_MUX_MSK) >> FB_MUX_OFF)
#define GET_LMK04832_FB_MUX_EN(x) (((x) & FB_MUX_EN_MSK) >> FB_MUX_EN_OFF)
#define SET_LMK04832_PLL2_RCLK_MUX(p, f) (p) = ((p) & ~PLL2_RCLK_MUX_MSK) | (((f) << PLL2_RCLK_MUX_OFF) & PLL2_RCLK_MUX_MSK)
#define SET_LMK04832_PLL2_NCLK_MUX(p, f) (p) = ((p) & ~PLL2_NCLK_MUX_MSK) | (((f) << PLL2_NCLK_MUX_OFF) & PLL2_NCLK_MUX_MSK)
#define SET_LMK04832_PLL1_NCLK_MUX(p, f) (p) = ((p) & ~PLL1_NCLK_MUX_MSK) | (((f) << PLL1_NCLK_MUX_OFF) & PLL1_NCLK_MUX_MSK)
#define SET_LMK04832_FB_MUX(p, f) (p) = ((p) & ~FB_MUX_MSK) | (((f) << FB_MUX_OFF) & FB_MUX_MSK)
#define SET_LMK04832_FB_MUX_EN(p, f) (p) = ((p) & ~FB_MUX_EN_MSK) | (((f) << FB_MUX_EN_OFF) & FB_MUX_EN_MSK)

#define MAKE_LMK04832_FB_CFG(pll2_rclk_mux, pll2_nclk_mux, pll1_nclk_mux, fb_mux, fb_mux_en) MAKE_LMK04832_REG_WR(FB_CFG, \
    (((pll2_rclk_mux) << PLL2_RCLK_MUX_OFF) & PLL2_RCLK_MUX_MSK) |  \
    (((pll2_nclk_mux) << PLL2_NCLK_MUX_OFF) & PLL2_NCLK_MUX_MSK) |  \
    (((pll1_nclk_mux) << PLL1_NCLK_MUX_OFF) & PLL1_NCLK_MUX_MSK) |  \
    (((fb_mux) << FB_MUX_OFF) & FB_MUX_MSK) |  \
    (((fb_mux_en) << FB_MUX_EN_OFF) & FB_MUX_EN_MSK))
// Register R320 [0x140] -- OSC_SYSREF_CFG

enum osc_sysref_cfg_fields_t {
    PLL1_PD_OFF = 0x7,
    PLL1_PD_MSK = 0x80,
    VCO_LDO_PD_OFF = 0x6,
    VCO_LDO_PD_MSK = 0x40,
    VCO_PD_OFF = 0x5,
    VCO_PD_MSK = 0x20,
    OSCIN_PD_OFF = 0x4,
    OSCIN_PD_MSK = 0x10,
    SYSREF_GBL_PD_OFF = 0x3,
    SYSREF_GBL_PD_MSK = 0x8,
    SYSREF_PD_OFF = 0x2,
    SYSREF_PD_MSK = 0x4,
    SYSREF_DDLY_PD_OFF = 0x1,
    SYSREF_DDLY_PD_MSK = 0x2,
    SYSREF_PLSR_PD_OFF = 0x0,
    SYSREF_PLSR_PD_MSK = 0x1,
};
#define GET_LMK04832_PLL1_PD(x) (((x) & PLL1_PD_MSK) >> PLL1_PD_OFF)
#define GET_LMK04832_VCO_LDO_PD(x) (((x) & VCO_LDO_PD_MSK) >> VCO_LDO_PD_OFF)
#define GET_LMK04832_VCO_PD(x) (((x) & VCO_PD_MSK) >> VCO_PD_OFF)
#define GET_LMK04832_OSCIN_PD(x) (((x) & OSCIN_PD_MSK) >> OSCIN_PD_OFF)
#define GET_LMK04832_SYSREF_GBL_PD(x) (((x) & SYSREF_GBL_PD_MSK) >> SYSREF_GBL_PD_OFF)
#define GET_LMK04832_SYSREF_PD(x) (((x) & SYSREF_PD_MSK) >> SYSREF_PD_OFF)
#define GET_LMK04832_SYSREF_DDLY_PD(x) (((x) & SYSREF_DDLY_PD_MSK) >> SYSREF_DDLY_PD_OFF)
#define GET_LMK04832_SYSREF_PLSR_PD(x) (((x) & SYSREF_PLSR_PD_MSK) >> SYSREF_PLSR_PD_OFF)
#define SET_LMK04832_PLL1_PD(p, f) (p) = ((p) & ~PLL1_PD_MSK) | (((f) << PLL1_PD_OFF) & PLL1_PD_MSK)
#define SET_LMK04832_VCO_LDO_PD(p, f) (p) = ((p) & ~VCO_LDO_PD_MSK) | (((f) << VCO_LDO_PD_OFF) & VCO_LDO_PD_MSK)
#define SET_LMK04832_VCO_PD(p, f) (p) = ((p) & ~VCO_PD_MSK) | (((f) << VCO_PD_OFF) & VCO_PD_MSK)
#define SET_LMK04832_OSCIN_PD(p, f) (p) = ((p) & ~OSCIN_PD_MSK) | (((f) << OSCIN_PD_OFF) & OSCIN_PD_MSK)
#define SET_LMK04832_SYSREF_GBL_PD(p, f) (p) = ((p) & ~SYSREF_GBL_PD_MSK) | (((f) << SYSREF_GBL_PD_OFF) & SYSREF_GBL_PD_MSK)
#define SET_LMK04832_SYSREF_PD(p, f) (p) = ((p) & ~SYSREF_PD_MSK) | (((f) << SYSREF_PD_OFF) & SYSREF_PD_MSK)
#define SET_LMK04832_SYSREF_DDLY_PD(p, f) (p) = ((p) & ~SYSREF_DDLY_PD_MSK) | (((f) << SYSREF_DDLY_PD_OFF) & SYSREF_DDLY_PD_MSK)
#define SET_LMK04832_SYSREF_PLSR_PD(p, f) (p) = ((p) & ~SYSREF_PLSR_PD_MSK) | (((f) << SYSREF_PLSR_PD_OFF) & SYSREF_PLSR_PD_MSK)

#define MAKE_LMK04832_OSC_SYSREF_CFG(pll1_pd, vco_ldo_pd, vco_pd, oscin_pd, sysref_gbl_pd, sysref_pd, sysref_ddly_pd, sysref_plsr_pd) MAKE_LMK04832_REG_WR(OSC_SYSREF_CFG, \
    (((pll1_pd) << PLL1_PD_OFF) & PLL1_PD_MSK) |  \
    (((vco_ldo_pd) << VCO_LDO_PD_OFF) & VCO_LDO_PD_MSK) |  \
    (((vco_pd) << VCO_PD_OFF) & VCO_PD_MSK) |  \
    (((oscin_pd) << OSCIN_PD_OFF) & OSCIN_PD_MSK) |  \
    (((sysref_gbl_pd) << SYSREF_GBL_PD_OFF) & SYSREF_GBL_PD_MSK) |  \
    (((sysref_pd) << SYSREF_PD_OFF) & SYSREF_PD_MSK) |  \
    (((sysref_ddly_pd) << SYSREF_DDLY_PD_OFF) & SYSREF_DDLY_PD_MSK) |  \
    (((sysref_plsr_pd) << SYSREF_PLSR_PD_OFF) & SYSREF_PLSR_PD_MSK))
// Register R325 [0x145] -- PLL1_2_SYNC
enum pll1r_sync_src_options {
    PLL1R_SYNC_SRC_SYNC_PIN = 1,
    PLL1R_SYNC_SRC_CLKIN0 = 2,
};

enum pll1_2_sync_fields_t {
    PLL1R_SYNC_EN_OFF = 0x6,
    PLL1R_SYNC_EN_MSK = 0x40,
    PLL1R_SYNC_SRC_OFF = 0x4,
    PLL1R_SYNC_SRC_MSK = 0x30,
    PLL2R_SYNC_EN_OFF = 0x3,
    PLL2R_SYNC_EN_MSK = 0x8,
};
#define GET_LMK04832_PLL1R_SYNC_EN(x) (((x) & PLL1R_SYNC_EN_MSK) >> PLL1R_SYNC_EN_OFF)
#define GET_LMK04832_PLL1R_SYNC_SRC(x) (((x) & PLL1R_SYNC_SRC_MSK) >> PLL1R_SYNC_SRC_OFF)
#define GET_LMK04832_PLL2R_SYNC_EN(x) (((x) & PLL2R_SYNC_EN_MSK) >> PLL2R_SYNC_EN_OFF)
#define SET_LMK04832_PLL1R_SYNC_EN(p, f) (p) = ((p) & ~PLL1R_SYNC_EN_MSK) | (((f) << PLL1R_SYNC_EN_OFF) & PLL1R_SYNC_EN_MSK)
#define SET_LMK04832_PLL1R_SYNC_SRC(p, f) (p) = ((p) & ~PLL1R_SYNC_SRC_MSK) | (((f) << PLL1R_SYNC_SRC_OFF) & PLL1R_SYNC_SRC_MSK)
#define SET_LMK04832_PLL2R_SYNC_EN(p, f) (p) = ((p) & ~PLL2R_SYNC_EN_MSK) | (((f) << PLL2R_SYNC_EN_OFF) & PLL2R_SYNC_EN_MSK)

#define MAKE_LMK04832_PLL1_2_SYNC(pll1r_sync_en, pll1r_sync_src, pll2r_sync_en) MAKE_LMK04832_REG_WR(PLL1_2_SYNC, \
    (((pll1r_sync_en) << PLL1R_SYNC_EN_OFF) & PLL1R_SYNC_EN_MSK) |  \
    (((pll1r_sync_src) << PLL1R_SYNC_SRC_OFF) & PLL1R_SYNC_SRC_MSK) |  \
    (((pll2r_sync_en) << PLL2R_SYNC_EN_OFF) & PLL2R_SYNC_EN_MSK))
// Register R326 [0x146] -- CLKIN_CFG
enum clkin_sel_pin_pol_options {
    CLKIN_SEL_PIN_POL_ACTIVE_HIGH = 0,
    CLKIN_SEL_PIN_POL_ACTIVE_LOW = 1,
};
enum clkin2_type_options {
    CLKIN2_TYPE_BIPOLAR = 0,
    CLKIN2_TYPE_MOS = 1,
};
enum clkin1_type_options {
    CLKIN1_TYPE_BIPOLAR = 0,
    CLKIN1_TYPE_MOS = 1,
};
enum clkin0_type_options {
    CLKIN0_TYPE_BIPOLAR = 0,
    CLKIN0_TYPE_MOS = 1,
};

enum clkin_cfg_fields_t {
    CLKIN_SEL_PIN_EN_OFF = 0x7,
    CLKIN_SEL_PIN_EN_MSK = 0x80,
    CLKIN_SEL_PIN_POL_OFF = 0x6,
    CLKIN_SEL_PIN_POL_MSK = 0x40,
    CLKIN2_EN_OFF = 0x5,
    CLKIN2_EN_MSK = 0x20,
    CLKIN1_EN_OFF = 0x4,
    CLKIN1_EN_MSK = 0x10,
    CLKIN0_EN_OFF = 0x3,
    CLKIN0_EN_MSK = 0x8,
    CLKIN2_TYPE_OFF = 0x2,
    CLKIN2_TYPE_MSK = 0x4,
    CLKIN1_TYPE_OFF = 0x1,
    CLKIN1_TYPE_MSK = 0x2,
    CLKIN0_TYPE_OFF = 0x0,
    CLKIN0_TYPE_MSK = 0x1,
};
#define GET_LMK04832_CLKIN_SEL_PIN_EN(x) (((x) & CLKIN_SEL_PIN_EN_MSK) >> CLKIN_SEL_PIN_EN_OFF)
#define GET_LMK04832_CLKIN_SEL_PIN_POL(x) (((x) & CLKIN_SEL_PIN_POL_MSK) >> CLKIN_SEL_PIN_POL_OFF)
#define GET_LMK04832_CLKIN2_EN(x) (((x) & CLKIN2_EN_MSK) >> CLKIN2_EN_OFF)
#define GET_LMK04832_CLKIN1_EN(x) (((x) & CLKIN1_EN_MSK) >> CLKIN1_EN_OFF)
#define GET_LMK04832_CLKIN0_EN(x) (((x) & CLKIN0_EN_MSK) >> CLKIN0_EN_OFF)
#define GET_LMK04832_CLKIN2_TYPE(x) (((x) & CLKIN2_TYPE_MSK) >> CLKIN2_TYPE_OFF)
#define GET_LMK04832_CLKIN1_TYPE(x) (((x) & CLKIN1_TYPE_MSK) >> CLKIN1_TYPE_OFF)
#define GET_LMK04832_CLKIN0_TYPE(x) (((x) & CLKIN0_TYPE_MSK) >> CLKIN0_TYPE_OFF)
#define SET_LMK04832_CLKIN_SEL_PIN_EN(p, f) (p) = ((p) & ~CLKIN_SEL_PIN_EN_MSK) | (((f) << CLKIN_SEL_PIN_EN_OFF) & CLKIN_SEL_PIN_EN_MSK)
#define SET_LMK04832_CLKIN_SEL_PIN_POL(p, f) (p) = ((p) & ~CLKIN_SEL_PIN_POL_MSK) | (((f) << CLKIN_SEL_PIN_POL_OFF) & CLKIN_SEL_PIN_POL_MSK)
#define SET_LMK04832_CLKIN2_EN(p, f) (p) = ((p) & ~CLKIN2_EN_MSK) | (((f) << CLKIN2_EN_OFF) & CLKIN2_EN_MSK)
#define SET_LMK04832_CLKIN1_EN(p, f) (p) = ((p) & ~CLKIN1_EN_MSK) | (((f) << CLKIN1_EN_OFF) & CLKIN1_EN_MSK)
#define SET_LMK04832_CLKIN0_EN(p, f) (p) = ((p) & ~CLKIN0_EN_MSK) | (((f) << CLKIN0_EN_OFF) & CLKIN0_EN_MSK)
#define SET_LMK04832_CLKIN2_TYPE(p, f) (p) = ((p) & ~CLKIN2_TYPE_MSK) | (((f) << CLKIN2_TYPE_OFF) & CLKIN2_TYPE_MSK)
#define SET_LMK04832_CLKIN1_TYPE(p, f) (p) = ((p) & ~CLKIN1_TYPE_MSK) | (((f) << CLKIN1_TYPE_OFF) & CLKIN1_TYPE_MSK)
#define SET_LMK04832_CLKIN0_TYPE(p, f) (p) = ((p) & ~CLKIN0_TYPE_MSK) | (((f) << CLKIN0_TYPE_OFF) & CLKIN0_TYPE_MSK)

#define MAKE_LMK04832_CLKIN_CFG(clkin_sel_pin_en, clkin_sel_pin_pol, clkin2_en, clkin1_en, clkin0_en, clkin2_type, clkin1_type, clkin0_type) MAKE_LMK04832_REG_WR(CLKIN_CFG, \
    (((clkin_sel_pin_en) << CLKIN_SEL_PIN_EN_OFF) & CLKIN_SEL_PIN_EN_MSK) |  \
    (((clkin_sel_pin_pol) << CLKIN_SEL_PIN_POL_OFF) & CLKIN_SEL_PIN_POL_MSK) |  \
    (((clkin2_en) << CLKIN2_EN_OFF) & CLKIN2_EN_MSK) |  \
    (((clkin1_en) << CLKIN1_EN_OFF) & CLKIN1_EN_MSK) |  \
    (((clkin0_en) << CLKIN0_EN_OFF) & CLKIN0_EN_MSK) |  \
    (((clkin2_type) << CLKIN2_TYPE_OFF) & CLKIN2_TYPE_MSK) |  \
    (((clkin1_type) << CLKIN1_TYPE_OFF) & CLKIN1_TYPE_MSK) |  \
    (((clkin0_type) << CLKIN0_TYPE_OFF) & CLKIN0_TYPE_MSK))
// Register R327 [0x147] -- CLKIN_TYPE
enum clkin_sel_manual_options {
    CLKIN_SEL_MANUAL_CLKIN0 = 0,
    CLKIN_SEL_MANUAL_CLKIN1 = 1,
    CLKIN_SEL_MANUAL_CLKIN2 = 2,
    CLKIN_SEL_MANUAL_HOLDOVER = 3,
};
enum clkin1_demux_options {
    CLKIN1_DEMUX_FIN = 0,
    CLKIN1_DEMUX_FEEDBACK_MUX = 1,
    CLKIN1_DEMUX_PLL1 = 2,
    CLKIN1_DEMUX_PD = 3,
};
enum clkin0_demux_options {
    CLKIN0_DEMUX_SYSREF_MUX = 0,
    CLKIN0_DEMUX_PLL1 = 2,
    CLKIN0_DEMUX_PD = 3,
};

enum clkin_type_fields_t {
    CLKIN_SEL_AUTO_REVERT_EN_OFF = 0x7,
    CLKIN_SEL_AUTO_REVERT_EN_MSK = 0x80,
    CLKIN_SEL_AUTO_EN_OFF = 0x6,
    CLKIN_SEL_AUTO_EN_MSK = 0x40,
    CLKIN_SEL_MANUAL_OFF = 0x4,
    CLKIN_SEL_MANUAL_MSK = 0x30,
    CLKIN1_DEMUX_OFF = 0x2,
    CLKIN1_DEMUX_MSK = 0xc,
    CLKIN0_DEMUX_OFF = 0x0,
    CLKIN0_DEMUX_MSK = 0x3,
};
#define GET_LMK04832_CLKIN_SEL_AUTO_REVERT_EN(x) (((x) & CLKIN_SEL_AUTO_REVERT_EN_MSK) >> CLKIN_SEL_AUTO_REVERT_EN_OFF)
#define GET_LMK04832_CLKIN_SEL_AUTO_EN(x) (((x) & CLKIN_SEL_AUTO_EN_MSK) >> CLKIN_SEL_AUTO_EN_OFF)
#define GET_LMK04832_CLKIN_SEL_MANUAL(x) (((x) & CLKIN_SEL_MANUAL_MSK) >> CLKIN_SEL_MANUAL_OFF)
#define GET_LMK04832_CLKIN1_DEMUX(x) (((x) & CLKIN1_DEMUX_MSK) >> CLKIN1_DEMUX_OFF)
#define GET_LMK04832_CLKIN0_DEMUX(x) (((x) & CLKIN0_DEMUX_MSK) >> CLKIN0_DEMUX_OFF)
#define SET_LMK04832_CLKIN_SEL_AUTO_REVERT_EN(p, f) (p) = ((p) & ~CLKIN_SEL_AUTO_REVERT_EN_MSK) | (((f) << CLKIN_SEL_AUTO_REVERT_EN_OFF) & CLKIN_SEL_AUTO_REVERT_EN_MSK)
#define SET_LMK04832_CLKIN_SEL_AUTO_EN(p, f) (p) = ((p) & ~CLKIN_SEL_AUTO_EN_MSK) | (((f) << CLKIN_SEL_AUTO_EN_OFF) & CLKIN_SEL_AUTO_EN_MSK)
#define SET_LMK04832_CLKIN_SEL_MANUAL(p, f) (p) = ((p) & ~CLKIN_SEL_MANUAL_MSK) | (((f) << CLKIN_SEL_MANUAL_OFF) & CLKIN_SEL_MANUAL_MSK)
#define SET_LMK04832_CLKIN1_DEMUX(p, f) (p) = ((p) & ~CLKIN1_DEMUX_MSK) | (((f) << CLKIN1_DEMUX_OFF) & CLKIN1_DEMUX_MSK)
#define SET_LMK04832_CLKIN0_DEMUX(p, f) (p) = ((p) & ~CLKIN0_DEMUX_MSK) | (((f) << CLKIN0_DEMUX_OFF) & CLKIN0_DEMUX_MSK)

#define MAKE_LMK04832_CLKIN_TYPE(clkin_sel_auto_revert_en, clkin_sel_auto_en, clkin_sel_manual, clkin1_demux, clkin0_demux) MAKE_LMK04832_REG_WR(CLKIN_TYPE, \
    (((clkin_sel_auto_revert_en) << CLKIN_SEL_AUTO_REVERT_EN_OFF) & CLKIN_SEL_AUTO_REVERT_EN_MSK) |  \
    (((clkin_sel_auto_en) << CLKIN_SEL_AUTO_EN_OFF) & CLKIN_SEL_AUTO_EN_MSK) |  \
    (((clkin_sel_manual) << CLKIN_SEL_MANUAL_OFF) & CLKIN_SEL_MANUAL_MSK) |  \
    (((clkin1_demux) << CLKIN1_DEMUX_OFF) & CLKIN1_DEMUX_MSK) |  \
    (((clkin0_demux) << CLKIN0_DEMUX_OFF) & CLKIN0_DEMUX_MSK))
// Register R330 [0x14a] -- RESET_CFG
enum reset_mux_options {
    RESET_MUX_SPI_READBACK = 6,
};
enum reset_type_options {
    RESET_TYPE_OUTPUT_PUSH_PULL = 3,
};

enum reset_cfg_fields_t {
    RESET_MUX_OFF = 0x3,
    RESET_MUX_MSK = 0x38,
    RESET_TYPE_OFF = 0x0,
    RESET_TYPE_MSK = 0x7,
};
#define GET_LMK04832_RESET_MUX(x) (((x) & RESET_MUX_MSK) >> RESET_MUX_OFF)
#define GET_LMK04832_RESET_TYPE(x) (((x) & RESET_TYPE_MSK) >> RESET_TYPE_OFF)
#define SET_LMK04832_RESET_MUX(p, f) (p) = ((p) & ~RESET_MUX_MSK) | (((f) << RESET_MUX_OFF) & RESET_MUX_MSK)
#define SET_LMK04832_RESET_TYPE(p, f) (p) = ((p) & ~RESET_TYPE_MSK) | (((f) << RESET_TYPE_OFF) & RESET_TYPE_MSK)

#define MAKE_LMK04832_RESET_CFG(reset_mux, reset_type) MAKE_LMK04832_REG_WR(RESET_CFG, \
    (((reset_mux) << RESET_MUX_OFF) & RESET_MUX_MSK) |  \
    (((reset_type) << RESET_TYPE_OFF) & RESET_TYPE_MSK))
// Register R339 [0x153] -- CLKIN0_R_HI

enum clkin0_r_hi_fields_t {
    CLKIN0_R_HI_OFF = 0x0,
    CLKIN0_R_HI_MSK = 0x3f,
};
#define GET_LMK04832_CLKIN0_R_HI(x) (((x) & CLKIN0_R_HI_MSK) >> CLKIN0_R_HI_OFF)
#define SET_LMK04832_CLKIN0_R_HI(p, f) (p) = ((p) & ~CLKIN0_R_HI_MSK) | (((f) << CLKIN0_R_HI_OFF) & CLKIN0_R_HI_MSK)

#define MAKE_LMK04832_CLKIN0_R_HI(clkin0_r_hi) MAKE_LMK04832_REG_WR(CLKIN0_R_HI, \
    (((clkin0_r_hi) << CLKIN0_R_HI_OFF) & CLKIN0_R_HI_MSK))
// Register R340 [0x154] -- CLKIN0_R_LOW

enum clkin0_r_low_fields_t {
    CLKIN0_R_LOW_OFF = 0x0,
    CLKIN0_R_LOW_MSK = 0xff,
};
#define GET_LMK04832_CLKIN0_R_LOW(x) (((x) & CLKIN0_R_LOW_MSK) >> CLKIN0_R_LOW_OFF)
#define SET_LMK04832_CLKIN0_R_LOW(p, f) (p) = ((p) & ~CLKIN0_R_LOW_MSK) | (((f) << CLKIN0_R_LOW_OFF) & CLKIN0_R_LOW_MSK)

#define MAKE_LMK04832_CLKIN0_R_LOW(clkin0_r_low) MAKE_LMK04832_REG_WR(CLKIN0_R_LOW, \
    (((clkin0_r_low) << CLKIN0_R_LOW_OFF) & CLKIN0_R_LOW_MSK))
// Register R341 [0x155] -- CLKIN1_R_HI

enum clkin1_r_hi_fields_t {
    CLKIN1_R_HI_OFF = 0x0,
    CLKIN1_R_HI_MSK = 0x3f,
};
#define GET_LMK04832_CLKIN1_R_HI(x) (((x) & CLKIN1_R_HI_MSK) >> CLKIN1_R_HI_OFF)
#define SET_LMK04832_CLKIN1_R_HI(p, f) (p) = ((p) & ~CLKIN1_R_HI_MSK) | (((f) << CLKIN1_R_HI_OFF) & CLKIN1_R_HI_MSK)

#define MAKE_LMK04832_CLKIN1_R_HI(clkin1_r_hi) MAKE_LMK04832_REG_WR(CLKIN1_R_HI, \
    (((clkin1_r_hi) << CLKIN1_R_HI_OFF) & CLKIN1_R_HI_MSK))
// Register R342 [0x156] -- CLKIN1_R_LOW

enum clkin1_r_low_fields_t {
    CLKIN1_R_LOW_OFF = 0x0,
    CLKIN1_R_LOW_MSK = 0xff,
};
#define GET_LMK04832_CLKIN1_R_LOW(x) (((x) & CLKIN1_R_LOW_MSK) >> CLKIN1_R_LOW_OFF)
#define SET_LMK04832_CLKIN1_R_LOW(p, f) (p) = ((p) & ~CLKIN1_R_LOW_MSK) | (((f) << CLKIN1_R_LOW_OFF) & CLKIN1_R_LOW_MSK)

#define MAKE_LMK04832_CLKIN1_R_LOW(clkin1_r_low) MAKE_LMK04832_REG_WR(CLKIN1_R_LOW, \
    (((clkin1_r_low) << CLKIN1_R_LOW_OFF) & CLKIN1_R_LOW_MSK))
// Register R343 [0x157] -- CLKIN2_R_HI

enum clkin2_r_hi_fields_t {
    CLKIN2_R_HI_OFF = 0x0,
    CLKIN2_R_HI_MSK = 0x3f,
};
#define GET_LMK04832_CLKIN2_R_HI(x) (((x) & CLKIN2_R_HI_MSK) >> CLKIN2_R_HI_OFF)
#define SET_LMK04832_CLKIN2_R_HI(p, f) (p) = ((p) & ~CLKIN2_R_HI_MSK) | (((f) << CLKIN2_R_HI_OFF) & CLKIN2_R_HI_MSK)

#define MAKE_LMK04832_CLKIN2_R_HI(clkin2_r_hi) MAKE_LMK04832_REG_WR(CLKIN2_R_HI, \
    (((clkin2_r_hi) << CLKIN2_R_HI_OFF) & CLKIN2_R_HI_MSK))
// Register R344 [0x158] -- CLKIN2_R_LOW

enum clkin2_r_low_fields_t {
    CLKIN2_R_LOW_OFF = 0x0,
    CLKIN2_R_LOW_MSK = 0xff,
};
#define GET_LMK04832_CLKIN2_R_LOW(x) (((x) & CLKIN2_R_LOW_MSK) >> CLKIN2_R_LOW_OFF)
#define SET_LMK04832_CLKIN2_R_LOW(p, f) (p) = ((p) & ~CLKIN2_R_LOW_MSK) | (((f) << CLKIN2_R_LOW_OFF) & CLKIN2_R_LOW_MSK)

#define MAKE_LMK04832_CLKIN2_R_LOW(clkin2_r_low) MAKE_LMK04832_REG_WR(CLKIN2_R_LOW, \
    (((clkin2_r_low) << CLKIN2_R_LOW_OFF) & CLKIN2_R_LOW_MSK))
// Register R345 [0x159] -- PLL1_N_HI

enum pll1_n_hi_fields_t {
    PLL1_N_HI_OFF = 0x0,
    PLL1_N_HI_MSK = 0x3f,
};
#define GET_LMK04832_PLL1_N_HI(x) (((x) & PLL1_N_HI_MSK) >> PLL1_N_HI_OFF)
#define SET_LMK04832_PLL1_N_HI(p, f) (p) = ((p) & ~PLL1_N_HI_MSK) | (((f) << PLL1_N_HI_OFF) & PLL1_N_HI_MSK)

#define MAKE_LMK04832_PLL1_N_HI(pll1_n_hi) MAKE_LMK04832_REG_WR(PLL1_N_HI, \
    (((pll1_n_hi) << PLL1_N_HI_OFF) & PLL1_N_HI_MSK))
// Register R346 [0x15a] -- PLL1_N_LOW

enum pll1_n_low_fields_t {
    PLL1_N_LOW_OFF = 0x0,
    PLL1_N_LOW_MSK = 0xff,
};
#define GET_LMK04832_PLL1_N_LOW(x) (((x) & PLL1_N_LOW_MSK) >> PLL1_N_LOW_OFF)
#define SET_LMK04832_PLL1_N_LOW(p, f) (p) = ((p) & ~PLL1_N_LOW_MSK) | (((f) << PLL1_N_LOW_OFF) & PLL1_N_LOW_MSK)

#define MAKE_LMK04832_PLL1_N_LOW(pll1_n_low) MAKE_LMK04832_REG_WR(PLL1_N_LOW, \
    (((pll1_n_low) << PLL1_N_LOW_OFF) & PLL1_N_LOW_MSK))
// Register R348 [0x15c] -- PLL1_DLD_CNT_HI

enum pll1_dld_cnt_hi_fields_t {
    PLL1_DLD_CNT_OFF = 0x0,
    PLL1_DLD_CNT_MSK = 0x3f,
};
#define GET_LMK04832_PLL1_DLD_CNT(x) (((x) & PLL1_DLD_CNT_MSK) >> PLL1_DLD_CNT_OFF)
#define SET_LMK04832_PLL1_DLD_CNT(p, f) (p) = ((p) & ~PLL1_DLD_CNT_MSK) | (((f) << PLL1_DLD_CNT_OFF) & PLL1_DLD_CNT_MSK)

#define MAKE_LMK04832_PLL1_DLD_CNT_HI(pll1_dld_cnt) MAKE_LMK04832_REG_WR(PLL1_DLD_CNT_HI, \
    (((pll1_dld_cnt) << PLL1_DLD_CNT_OFF) & PLL1_DLD_CNT_MSK))
// Register R349 [0x15d] -- PLL1_DLD_CNT_LOW

enum pll1_dld_cnt_low_fields_t {
    PLL1_DLD_CNT_LOW_OFF = 0x0,
    PLL1_DLD_CNT_LOW_MSK = 0xff,
};
#define GET_LMK04832_PLL1_DLD_CNT_LOW(x) (((x) & PLL1_DLD_CNT_LOW_MSK) >> PLL1_DLD_CNT_LOW_OFF)
#define SET_LMK04832_PLL1_DLD_CNT_LOW(p, f) (p) = ((p) & ~PLL1_DLD_CNT_LOW_MSK) | (((f) << PLL1_DLD_CNT_LOW_OFF) & PLL1_DLD_CNT_LOW_MSK)

#define MAKE_LMK04832_PLL1_DLD_CNT_LOW(pll1_dld_cnt_low) MAKE_LMK04832_REG_WR(PLL1_DLD_CNT_LOW, \
    (((pll1_dld_cnt_low) << PLL1_DLD_CNT_LOW_OFF) & PLL1_DLD_CNT_LOW_MSK))
// Register R352 [0x160] -- PLL2_R_HI

enum pll2_r_hi_fields_t {
    PLL2_R_HI_OFF = 0x0,
    PLL2_R_HI_MSK = 0xf,
};
#define GET_LMK04832_PLL2_R_HI(x) (((x) & PLL2_R_HI_MSK) >> PLL2_R_HI_OFF)
#define SET_LMK04832_PLL2_R_HI(p, f) (p) = ((p) & ~PLL2_R_HI_MSK) | (((f) << PLL2_R_HI_OFF) & PLL2_R_HI_MSK)

#define MAKE_LMK04832_PLL2_R_HI(pll2_r_hi) MAKE_LMK04832_REG_WR(PLL2_R_HI, \
    (((pll2_r_hi) << PLL2_R_HI_OFF) & PLL2_R_HI_MSK))
// Register R353 [0x161] -- PLL2_R_LOW

enum pll2_r_low_fields_t {
    PLL2_R_LOW_OFF = 0x0,
    PLL2_R_LOW_MSK = 0xff,
};
#define GET_LMK04832_PLL2_R_LOW(x) (((x) & PLL2_R_LOW_MSK) >> PLL2_R_LOW_OFF)
#define SET_LMK04832_PLL2_R_LOW(p, f) (p) = ((p) & ~PLL2_R_LOW_MSK) | (((f) << PLL2_R_LOW_OFF) & PLL2_R_LOW_MSK)

#define MAKE_LMK04832_PLL2_R_LOW(pll2_r_low) MAKE_LMK04832_REG_WR(PLL2_R_LOW, \
    (((pll2_r_low) << PLL2_R_LOW_OFF) & PLL2_R_LOW_MSK))
// Register R354 [0x162] -- PLL2_FUNC
enum oscin_freq_options {
    OSCIN_FREQ_OSC_FREQ_0_63 = 0,
    OSCIN_FREQ_OSC_FREQ_63_127 = 1,
    OSCIN_FREQ_OSC_FREQ_127_255 = 2,
    OSCIN_FREQ_OSC_FREQ_255_500 = 4,
};

enum pll2_func_fields_t {
    PLL2_P_OFF = 0x5,
    PLL2_P_MSK = 0xe0,
    OSCIN_FREQ_OFF = 0x2,
    OSCIN_FREQ_MSK = 0x1c,
    PLL2_REF_2X_EN_OFF = 0x0,
    PLL2_REF_2X_EN_MSK = 0x1,
};
#define GET_LMK04832_PLL2_P(x) (((x) & PLL2_P_MSK) >> PLL2_P_OFF)
#define GET_LMK04832_OSCIN_FREQ(x) (((x) & OSCIN_FREQ_MSK) >> OSCIN_FREQ_OFF)
#define GET_LMK04832_PLL2_REF_2X_EN(x) (((x) & PLL2_REF_2X_EN_MSK) >> PLL2_REF_2X_EN_OFF)
#define SET_LMK04832_PLL2_P(p, f) (p) = ((p) & ~PLL2_P_MSK) | (((f) << PLL2_P_OFF) & PLL2_P_MSK)
#define SET_LMK04832_OSCIN_FREQ(p, f) (p) = ((p) & ~OSCIN_FREQ_MSK) | (((f) << OSCIN_FREQ_OFF) & OSCIN_FREQ_MSK)
#define SET_LMK04832_PLL2_REF_2X_EN(p, f) (p) = ((p) & ~PLL2_REF_2X_EN_MSK) | (((f) << PLL2_REF_2X_EN_OFF) & PLL2_REF_2X_EN_MSK)

#define MAKE_LMK04832_PLL2_FUNC(pll2_p, oscin_freq, pll2_ref_2x_en) MAKE_LMK04832_REG_WR(PLL2_FUNC, \
    (((pll2_p) << PLL2_P_OFF) & PLL2_P_MSK) |  \
    (((oscin_freq) << OSCIN_FREQ_OFF) & OSCIN_FREQ_MSK) |  \
    (((pll2_ref_2x_en) << PLL2_REF_2X_EN_OFF) & PLL2_REF_2X_EN_MSK))
// Register R355 [0x163] -- PLL2_N_CAL_HI

enum pll2_n_cal_hi_fields_t {
    PLL2_N_CAL_HI_OFF = 0x0,
    PLL2_N_CAL_HI_MSK = 0x3,
};
#define GET_LMK04832_PLL2_N_CAL_HI(x) (((x) & PLL2_N_CAL_HI_MSK) >> PLL2_N_CAL_HI_OFF)
#define SET_LMK04832_PLL2_N_CAL_HI(p, f) (p) = ((p) & ~PLL2_N_CAL_HI_MSK) | (((f) << PLL2_N_CAL_HI_OFF) & PLL2_N_CAL_HI_MSK)

#define MAKE_LMK04832_PLL2_N_CAL_HI(pll2_n_cal_hi) MAKE_LMK04832_REG_WR(PLL2_N_CAL_HI, \
    (((pll2_n_cal_hi) << PLL2_N_CAL_HI_OFF) & PLL2_N_CAL_HI_MSK))
// Register R356 [0x164] -- PLL2_N_CAL_MID

enum pll2_n_cal_mid_fields_t {
    PLL2_N_CAL_MID_OFF = 0x0,
    PLL2_N_CAL_MID_MSK = 0xff,
};
#define GET_LMK04832_PLL2_N_CAL_MID(x) (((x) & PLL2_N_CAL_MID_MSK) >> PLL2_N_CAL_MID_OFF)
#define SET_LMK04832_PLL2_N_CAL_MID(p, f) (p) = ((p) & ~PLL2_N_CAL_MID_MSK) | (((f) << PLL2_N_CAL_MID_OFF) & PLL2_N_CAL_MID_MSK)

#define MAKE_LMK04832_PLL2_N_CAL_MID(pll2_n_cal_mid) MAKE_LMK04832_REG_WR(PLL2_N_CAL_MID, \
    (((pll2_n_cal_mid) << PLL2_N_CAL_MID_OFF) & PLL2_N_CAL_MID_MSK))
// Register R357 [0x165] -- PLL2_N_CAL_LOW

enum pll2_n_cal_low_fields_t {
    PLL2_N_CAL_LOW_OFF = 0x0,
    PLL2_N_CAL_LOW_MSK = 0xff,
};
#define GET_LMK04832_PLL2_N_CAL_LOW(x) (((x) & PLL2_N_CAL_LOW_MSK) >> PLL2_N_CAL_LOW_OFF)
#define SET_LMK04832_PLL2_N_CAL_LOW(p, f) (p) = ((p) & ~PLL2_N_CAL_LOW_MSK) | (((f) << PLL2_N_CAL_LOW_OFF) & PLL2_N_CAL_LOW_MSK)

#define MAKE_LMK04832_PLL2_N_CAL_LOW(pll2_n_cal_low) MAKE_LMK04832_REG_WR(PLL2_N_CAL_LOW, \
    (((pll2_n_cal_low) << PLL2_N_CAL_LOW_OFF) & PLL2_N_CAL_LOW_MSK))
// Register R358 [0x166] -- PLL2_N_HI

enum pll2_n_hi_fields_t {
    PLL2_N_HI_OFF = 0x0,
    PLL2_N_HI_MSK = 0x3,
};
#define GET_LMK04832_PLL2_N_HI(x) (((x) & PLL2_N_HI_MSK) >> PLL2_N_HI_OFF)
#define SET_LMK04832_PLL2_N_HI(p, f) (p) = ((p) & ~PLL2_N_HI_MSK) | (((f) << PLL2_N_HI_OFF) & PLL2_N_HI_MSK)

#define MAKE_LMK04832_PLL2_N_HI(pll2_n_hi) MAKE_LMK04832_REG_WR(PLL2_N_HI, \
    (((pll2_n_hi) << PLL2_N_HI_OFF) & PLL2_N_HI_MSK))
// Register R359 [0x167] -- PLL2_N_MID

enum pll2_n_mid_fields_t {
    PLL2_N_MID_OFF = 0x0,
    PLL2_N_MID_MSK = 0xff,
};
#define GET_LMK04832_PLL2_N_MID(x) (((x) & PLL2_N_MID_MSK) >> PLL2_N_MID_OFF)
#define SET_LMK04832_PLL2_N_MID(p, f) (p) = ((p) & ~PLL2_N_MID_MSK) | (((f) << PLL2_N_MID_OFF) & PLL2_N_MID_MSK)

#define MAKE_LMK04832_PLL2_N_MID(pll2_n_mid) MAKE_LMK04832_REG_WR(PLL2_N_MID, \
    (((pll2_n_mid) << PLL2_N_MID_OFF) & PLL2_N_MID_MSK))
// Register R360 [0x168] -- PLL2_N_LOW

enum pll2_n_low_fields_t {
    PLL2_N_LOW_OFF = 0x0,
    PLL2_N_LOW_MSK = 0xff,
};
#define GET_LMK04832_PLL2_N_LOW(x) (((x) & PLL2_N_LOW_MSK) >> PLL2_N_LOW_OFF)
#define SET_LMK04832_PLL2_N_LOW(p, f) (p) = ((p) & ~PLL2_N_LOW_MSK) | (((f) << PLL2_N_LOW_OFF) & PLL2_N_LOW_MSK)

#define MAKE_LMK04832_PLL2_N_LOW(pll2_n_low) MAKE_LMK04832_REG_WR(PLL2_N_LOW, \
    (((pll2_n_low) << PLL2_N_LOW_OFF) & PLL2_N_LOW_MSK))
// Register R371 [0x173] -- PLL2_PD
enum pll2_pd_reserved_options {
    PLL2_PD_RESERVED_PLL2_PD_RESERVED = 16,
};

enum pll2_pd_fields_t {
    PLL2_PRE_PD_OFF = 0x6,
    PLL2_PRE_PD_MSK = 0x40,
    PLL2_PD_OFF = 0x5,
    PLL2_PD_MSK = 0x20,
    PLL2_PD_RESERVED_OFF = 0x0,
    PLL2_PD_RESERVED_MSK = 0x1f,
};
#define GET_LMK04832_PLL2_PRE_PD(x) (((x) & PLL2_PRE_PD_MSK) >> PLL2_PRE_PD_OFF)
#define GET_LMK04832_PLL2_PD(x) (((x) & PLL2_PD_MSK) >> PLL2_PD_OFF)
#define GET_LMK04832_PLL2_PD_RESERVED(x) (((x) & PLL2_PD_RESERVED_MSK) >> PLL2_PD_RESERVED_OFF)
#define SET_LMK04832_PLL2_PRE_PD(p, f) (p) = ((p) & ~PLL2_PRE_PD_MSK) | (((f) << PLL2_PRE_PD_OFF) & PLL2_PRE_PD_MSK)
#define SET_LMK04832_PLL2_PD(p, f) (p) = ((p) & ~PLL2_PD_MSK) | (((f) << PLL2_PD_OFF) & PLL2_PD_MSK)
#define SET_LMK04832_PLL2_PD_RESERVED(p, f) (p) = ((p) & ~PLL2_PD_RESERVED_MSK) | (((f) << PLL2_PD_RESERVED_OFF) & PLL2_PD_RESERVED_MSK)

#define MAKE_LMK04832_PLL2_PD(pll2_pre_pd, pll2_pd, pll2_pd_reserved) MAKE_LMK04832_REG_WR(PLL2_PD, \
    (((pll2_pre_pd) << PLL2_PRE_PD_OFF) & PLL2_PRE_PD_MSK) |  \
    (((pll2_pd) << PLL2_PD_OFF) & PLL2_PD_MSK) |  \
    (((pll2_pd_reserved) << PLL2_PD_RESERVED_OFF) & PLL2_PD_RESERVED_MSK))
