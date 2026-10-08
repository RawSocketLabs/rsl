enum m2_dsdr_usr_regs_t {
    RX_IFAMP_BP = 0x40,
    RX_H_BAND = 0x41,
    RX_FILTER_BANK = 0x42,
    RX_ATTN = 0x43,
    TX_H_BAND = 0x44,
    ANT_SEL = 0x45,
    RX_CHEN = 0x46,
    TX_CHEN = 0x47,
    RX_8KA_LNA = 0x48,
    RX_8KA_PA = 0x49,
    RX_8KB = 0x4a,
    TX_8KB = 0x4b,
    PA_2ND_BP = 0x4c,
};
#define MAKE_M2_DSDR_USR_REG_WR(a, v) (0x80000000 | ((a) << 24) | ((v) & 0xffffff))
#define MAKE_M2_DSDR_USR_REG_RD(a) (((a) << 24))
enum m2_dsdr_usr_rx_filt_opts_t {
    RX_FILT_OPTS_FILT_400_1000M = 0x0,
    RX_FILT_OPTS_FILT_1000_2000M = 0x1,
    RX_FILT_OPTS_FILT_2000_3500M = 0x2,
    RX_FILT_OPTS_FILT_2500_5000M = 0x3,
    RX_FILT_OPTS_FILT_3500_7100M = 0x4,
    RX_FILT_OPTS_AUTO_400_1000M = 0x8,
    RX_FILT_OPTS_AUTO_1000_2000M = 0x9,
    RX_FILT_OPTS_AUTO_2000_3500M = 0xa,
    RX_FILT_OPTS_AUTO_2500_5000M = 0xb,
    RX_FILT_OPTS_AUTO_3500_7100M = 0xc,
};
enum m2_dsdr_usr_ant_opts_t {
    ANT_OPTS_RX_TO_RX_AND_TX_TO_TRX = 0x0,
    ANT_OPTS_RX_TO_TRX_AND_TX_TERM = 0x1,
    ANT_OPTS_RX_TO_RX_AND_TX_TERM = 0x2,
    ANT_OPTS_RX_TX_LOOPBACK = 0x3,
    ANT_OPTS_TDD_DRIVEN_AUTO = 0x4,
};
enum m2_dsdr_usr_band_opts_t {
    BAND_OPTS_BAND_400_3500 = 0x0,
    BAND_OPTS_BAND_2200_7200 = 0x1,
    BAND_OPTS_BAND_AUTO_L = 0x2,
    BAND_OPTS_BAND_AUTO_H = 0x3,
};
enum m2_dsdr_usr_rxband_opts_t {
    RXBAND_OPTS_BAND_400_3500 = 0x0,
    RXBAND_OPTS_BAND_2200_7200 = 0x1,
    RXBAND_OPTS_BAND_1580_2760_BP = 0x2,
    RXBAND_OPTS_DISABLE = 0x3,
    RXBAND_OPTS_BAND_AUTO_L = 0x4,
    RXBAND_OPTS_BAND_AUTO_H = 0x5,
    RXBAND_OPTS_BAND_AUTO_BP = 0x6,
    RXBAND_OPTS_BAND_AUTO_DIS = 0x7,
};
// Register R64 [0x40] -- RX_IFAMP_BP

enum rx_ifamp_bp_fields_t {
    RX_IFAMP_BP_D_OFF = 0x3,
    RX_IFAMP_BP_D_MSK = 0x8,
    RX_IFAMP_BP_C_OFF = 0x2,
    RX_IFAMP_BP_C_MSK = 0x4,
    RX_IFAMP_BP_B_OFF = 0x1,
    RX_IFAMP_BP_B_MSK = 0x2,
    RX_IFAMP_BP_A_OFF = 0x0,
    RX_IFAMP_BP_A_MSK = 0x1,
};
#define GET_M2_DSDR_USR_RX_IFAMP_BP_D(x) (((x) & RX_IFAMP_BP_D_MSK) >> RX_IFAMP_BP_D_OFF)
#define GET_M2_DSDR_USR_RX_IFAMP_BP_C(x) (((x) & RX_IFAMP_BP_C_MSK) >> RX_IFAMP_BP_C_OFF)
#define GET_M2_DSDR_USR_RX_IFAMP_BP_B(x) (((x) & RX_IFAMP_BP_B_MSK) >> RX_IFAMP_BP_B_OFF)
#define GET_M2_DSDR_USR_RX_IFAMP_BP_A(x) (((x) & RX_IFAMP_BP_A_MSK) >> RX_IFAMP_BP_A_OFF)
#define SET_M2_DSDR_USR_RX_IFAMP_BP_D(p, f) (p) = ((p) & ~RX_IFAMP_BP_D_MSK) | (((f) << RX_IFAMP_BP_D_OFF) & RX_IFAMP_BP_D_MSK)
#define SET_M2_DSDR_USR_RX_IFAMP_BP_C(p, f) (p) = ((p) & ~RX_IFAMP_BP_C_MSK) | (((f) << RX_IFAMP_BP_C_OFF) & RX_IFAMP_BP_C_MSK)
#define SET_M2_DSDR_USR_RX_IFAMP_BP_B(p, f) (p) = ((p) & ~RX_IFAMP_BP_B_MSK) | (((f) << RX_IFAMP_BP_B_OFF) & RX_IFAMP_BP_B_MSK)
#define SET_M2_DSDR_USR_RX_IFAMP_BP_A(p, f) (p) = ((p) & ~RX_IFAMP_BP_A_MSK) | (((f) << RX_IFAMP_BP_A_OFF) & RX_IFAMP_BP_A_MSK)

#define MAKE_M2_DSDR_USR_RX_IFAMP_BP(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(RX_IFAMP_BP, \
    (((d) << RX_IFAMP_BP_D_OFF) & RX_IFAMP_BP_D_MSK) |  \
    (((c) << RX_IFAMP_BP_C_OFF) & RX_IFAMP_BP_C_MSK) |  \
    (((b) << RX_IFAMP_BP_B_OFF) & RX_IFAMP_BP_B_MSK) |  \
    (((a) << RX_IFAMP_BP_A_OFF) & RX_IFAMP_BP_A_MSK))
// Register R65 [0x41] -- RX_H_BAND
enum rx_h_band_d_options {
    RX_H_BAND_D_BAND_400_3500 = 0,
    RX_H_BAND_D_BAND_2200_7200 = 1,
    RX_H_BAND_D_BAND_1580_2760_BP = 2,
    RX_H_BAND_D_DISABLE = 3,
    RX_H_BAND_D_BAND_AUTO_L = 4,
    RX_H_BAND_D_BAND_AUTO_H = 5,
    RX_H_BAND_D_BAND_AUTO_BP = 6,
    RX_H_BAND_D_BAND_AUTO_DIS = 7,
};
enum rx_h_band_c_options {
    RX_H_BAND_C_BAND_400_3500 = 0,
    RX_H_BAND_C_BAND_2200_7200 = 1,
    RX_H_BAND_C_BAND_1580_2760_BP = 2,
    RX_H_BAND_C_DISABLE = 3,
    RX_H_BAND_C_BAND_AUTO_L = 4,
    RX_H_BAND_C_BAND_AUTO_H = 5,
    RX_H_BAND_C_BAND_AUTO_BP = 6,
    RX_H_BAND_C_BAND_AUTO_DIS = 7,
};
enum rx_h_band_b_options {
    RX_H_BAND_B_BAND_400_3500 = 0,
    RX_H_BAND_B_BAND_2200_7200 = 1,
    RX_H_BAND_B_BAND_1580_2760_BP = 2,
    RX_H_BAND_B_DISABLE = 3,
    RX_H_BAND_B_BAND_AUTO_L = 4,
    RX_H_BAND_B_BAND_AUTO_H = 5,
    RX_H_BAND_B_BAND_AUTO_BP = 6,
    RX_H_BAND_B_BAND_AUTO_DIS = 7,
};
enum rx_h_band_a_options {
    RX_H_BAND_A_BAND_400_3500 = 0,
    RX_H_BAND_A_BAND_2200_7200 = 1,
    RX_H_BAND_A_BAND_1580_2760_BP = 2,
    RX_H_BAND_A_DISABLE = 3,
    RX_H_BAND_A_BAND_AUTO_L = 4,
    RX_H_BAND_A_BAND_AUTO_H = 5,
    RX_H_BAND_A_BAND_AUTO_BP = 6,
    RX_H_BAND_A_BAND_AUTO_DIS = 7,
};

enum rx_h_band_fields_t {
    RX_H_BAND_D_OFF = 0xc,
    RX_H_BAND_D_MSK = 0x7000,
    RX_H_BAND_C_OFF = 0x8,
    RX_H_BAND_C_MSK = 0x700,
    RX_H_BAND_B_OFF = 0x4,
    RX_H_BAND_B_MSK = 0x70,
    RX_H_BAND_A_OFF = 0x0,
    RX_H_BAND_A_MSK = 0x7,
};
#define GET_M2_DSDR_USR_RX_H_BAND_D(x) (((x) & RX_H_BAND_D_MSK) >> RX_H_BAND_D_OFF)
#define GET_M2_DSDR_USR_RX_H_BAND_C(x) (((x) & RX_H_BAND_C_MSK) >> RX_H_BAND_C_OFF)
#define GET_M2_DSDR_USR_RX_H_BAND_B(x) (((x) & RX_H_BAND_B_MSK) >> RX_H_BAND_B_OFF)
#define GET_M2_DSDR_USR_RX_H_BAND_A(x) (((x) & RX_H_BAND_A_MSK) >> RX_H_BAND_A_OFF)
#define SET_M2_DSDR_USR_RX_H_BAND_D(p, f) (p) = ((p) & ~RX_H_BAND_D_MSK) | (((f) << RX_H_BAND_D_OFF) & RX_H_BAND_D_MSK)
#define SET_M2_DSDR_USR_RX_H_BAND_C(p, f) (p) = ((p) & ~RX_H_BAND_C_MSK) | (((f) << RX_H_BAND_C_OFF) & RX_H_BAND_C_MSK)
#define SET_M2_DSDR_USR_RX_H_BAND_B(p, f) (p) = ((p) & ~RX_H_BAND_B_MSK) | (((f) << RX_H_BAND_B_OFF) & RX_H_BAND_B_MSK)
#define SET_M2_DSDR_USR_RX_H_BAND_A(p, f) (p) = ((p) & ~RX_H_BAND_A_MSK) | (((f) << RX_H_BAND_A_OFF) & RX_H_BAND_A_MSK)

#define MAKE_M2_DSDR_USR_RX_H_BAND(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(RX_H_BAND, \
    (((d) << RX_H_BAND_D_OFF) & RX_H_BAND_D_MSK) |  \
    (((c) << RX_H_BAND_C_OFF) & RX_H_BAND_C_MSK) |  \
    (((b) << RX_H_BAND_B_OFF) & RX_H_BAND_B_MSK) |  \
    (((a) << RX_H_BAND_A_OFF) & RX_H_BAND_A_MSK))
// Register R66 [0x42] -- RX_FILTER_BANK
enum rx_filter_bank_d_options {
    RX_FILTER_BANK_D_FILT_400_1000M = 0,
    RX_FILTER_BANK_D_FILT_1000_2000M = 1,
    RX_FILTER_BANK_D_FILT_2000_3500M = 2,
    RX_FILTER_BANK_D_FILT_2500_5000M = 3,
    RX_FILTER_BANK_D_FILT_3500_7100M = 4,
    RX_FILTER_BANK_D_AUTO_400_1000M = 8,
    RX_FILTER_BANK_D_AUTO_1000_2000M = 9,
    RX_FILTER_BANK_D_AUTO_2000_3500M = 10,
    RX_FILTER_BANK_D_AUTO_2500_5000M = 11,
    RX_FILTER_BANK_D_AUTO_3500_7100M = 12,
};
enum rx_filter_bank_c_options {
    RX_FILTER_BANK_C_FILT_400_1000M = 0,
    RX_FILTER_BANK_C_FILT_1000_2000M = 1,
    RX_FILTER_BANK_C_FILT_2000_3500M = 2,
    RX_FILTER_BANK_C_FILT_2500_5000M = 3,
    RX_FILTER_BANK_C_FILT_3500_7100M = 4,
    RX_FILTER_BANK_C_AUTO_400_1000M = 8,
    RX_FILTER_BANK_C_AUTO_1000_2000M = 9,
    RX_FILTER_BANK_C_AUTO_2000_3500M = 10,
    RX_FILTER_BANK_C_AUTO_2500_5000M = 11,
    RX_FILTER_BANK_C_AUTO_3500_7100M = 12,
};
enum rx_filter_bank_b_options {
    RX_FILTER_BANK_B_FILT_400_1000M = 0,
    RX_FILTER_BANK_B_FILT_1000_2000M = 1,
    RX_FILTER_BANK_B_FILT_2000_3500M = 2,
    RX_FILTER_BANK_B_FILT_2500_5000M = 3,
    RX_FILTER_BANK_B_FILT_3500_7100M = 4,
    RX_FILTER_BANK_B_AUTO_400_1000M = 8,
    RX_FILTER_BANK_B_AUTO_1000_2000M = 9,
    RX_FILTER_BANK_B_AUTO_2000_3500M = 10,
    RX_FILTER_BANK_B_AUTO_2500_5000M = 11,
    RX_FILTER_BANK_B_AUTO_3500_7100M = 12,
};
enum rx_filter_bank_a_options {
    RX_FILTER_BANK_A_FILT_400_1000M = 0,
    RX_FILTER_BANK_A_FILT_1000_2000M = 1,
    RX_FILTER_BANK_A_FILT_2000_3500M = 2,
    RX_FILTER_BANK_A_FILT_2500_5000M = 3,
    RX_FILTER_BANK_A_FILT_3500_7100M = 4,
    RX_FILTER_BANK_A_AUTO_400_1000M = 8,
    RX_FILTER_BANK_A_AUTO_1000_2000M = 9,
    RX_FILTER_BANK_A_AUTO_2000_3500M = 10,
    RX_FILTER_BANK_A_AUTO_2500_5000M = 11,
    RX_FILTER_BANK_A_AUTO_3500_7100M = 12,
};

enum rx_filter_bank_fields_t {
    RX_FILTER_BANK_D_OFF = 0xc,
    RX_FILTER_BANK_D_MSK = 0xf000,
    RX_FILTER_BANK_C_OFF = 0x8,
    RX_FILTER_BANK_C_MSK = 0xf00,
    RX_FILTER_BANK_B_OFF = 0x4,
    RX_FILTER_BANK_B_MSK = 0xf0,
    RX_FILTER_BANK_A_OFF = 0x0,
    RX_FILTER_BANK_A_MSK = 0xf,
};
#define GET_M2_DSDR_USR_RX_FILTER_BANK_D(x) (((x) & RX_FILTER_BANK_D_MSK) >> RX_FILTER_BANK_D_OFF)
#define GET_M2_DSDR_USR_RX_FILTER_BANK_C(x) (((x) & RX_FILTER_BANK_C_MSK) >> RX_FILTER_BANK_C_OFF)
#define GET_M2_DSDR_USR_RX_FILTER_BANK_B(x) (((x) & RX_FILTER_BANK_B_MSK) >> RX_FILTER_BANK_B_OFF)
#define GET_M2_DSDR_USR_RX_FILTER_BANK_A(x) (((x) & RX_FILTER_BANK_A_MSK) >> RX_FILTER_BANK_A_OFF)
#define SET_M2_DSDR_USR_RX_FILTER_BANK_D(p, f) (p) = ((p) & ~RX_FILTER_BANK_D_MSK) | (((f) << RX_FILTER_BANK_D_OFF) & RX_FILTER_BANK_D_MSK)
#define SET_M2_DSDR_USR_RX_FILTER_BANK_C(p, f) (p) = ((p) & ~RX_FILTER_BANK_C_MSK) | (((f) << RX_FILTER_BANK_C_OFF) & RX_FILTER_BANK_C_MSK)
#define SET_M2_DSDR_USR_RX_FILTER_BANK_B(p, f) (p) = ((p) & ~RX_FILTER_BANK_B_MSK) | (((f) << RX_FILTER_BANK_B_OFF) & RX_FILTER_BANK_B_MSK)
#define SET_M2_DSDR_USR_RX_FILTER_BANK_A(p, f) (p) = ((p) & ~RX_FILTER_BANK_A_MSK) | (((f) << RX_FILTER_BANK_A_OFF) & RX_FILTER_BANK_A_MSK)

#define MAKE_M2_DSDR_USR_RX_FILTER_BANK(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(RX_FILTER_BANK, \
    (((d) << RX_FILTER_BANK_D_OFF) & RX_FILTER_BANK_D_MSK) |  \
    (((c) << RX_FILTER_BANK_C_OFF) & RX_FILTER_BANK_C_MSK) |  \
    (((b) << RX_FILTER_BANK_B_OFF) & RX_FILTER_BANK_B_MSK) |  \
    (((a) << RX_FILTER_BANK_A_OFF) & RX_FILTER_BANK_A_MSK))
// Register R67 [0x43] -- RX_ATTN

enum rx_attn_fields_t {
    RX_ATTN_D_OFF = 0xc,
    RX_ATTN_D_MSK = 0xf000,
    RX_ATTN_C_OFF = 0x8,
    RX_ATTN_C_MSK = 0xf00,
    RX_ATTN_B_OFF = 0x4,
    RX_ATTN_B_MSK = 0xf0,
    RX_ATTN_A_OFF = 0x0,
    RX_ATTN_A_MSK = 0xf,
};
#define GET_M2_DSDR_USR_RX_ATTN_D(x) (((x) & RX_ATTN_D_MSK) >> RX_ATTN_D_OFF)
#define GET_M2_DSDR_USR_RX_ATTN_C(x) (((x) & RX_ATTN_C_MSK) >> RX_ATTN_C_OFF)
#define GET_M2_DSDR_USR_RX_ATTN_B(x) (((x) & RX_ATTN_B_MSK) >> RX_ATTN_B_OFF)
#define GET_M2_DSDR_USR_RX_ATTN_A(x) (((x) & RX_ATTN_A_MSK) >> RX_ATTN_A_OFF)
#define SET_M2_DSDR_USR_RX_ATTN_D(p, f) (p) = ((p) & ~RX_ATTN_D_MSK) | (((f) << RX_ATTN_D_OFF) & RX_ATTN_D_MSK)
#define SET_M2_DSDR_USR_RX_ATTN_C(p, f) (p) = ((p) & ~RX_ATTN_C_MSK) | (((f) << RX_ATTN_C_OFF) & RX_ATTN_C_MSK)
#define SET_M2_DSDR_USR_RX_ATTN_B(p, f) (p) = ((p) & ~RX_ATTN_B_MSK) | (((f) << RX_ATTN_B_OFF) & RX_ATTN_B_MSK)
#define SET_M2_DSDR_USR_RX_ATTN_A(p, f) (p) = ((p) & ~RX_ATTN_A_MSK) | (((f) << RX_ATTN_A_OFF) & RX_ATTN_A_MSK)

#define MAKE_M2_DSDR_USR_RX_ATTN(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(RX_ATTN, \
    (((d) << RX_ATTN_D_OFF) & RX_ATTN_D_MSK) |  \
    (((c) << RX_ATTN_C_OFF) & RX_ATTN_C_MSK) |  \
    (((b) << RX_ATTN_B_OFF) & RX_ATTN_B_MSK) |  \
    (((a) << RX_ATTN_A_OFF) & RX_ATTN_A_MSK))
// Register R68 [0x44] -- TX_H_BAND
enum tx_h_band_d_options {
    TX_H_BAND_D_BAND_400_3500 = 0,
    TX_H_BAND_D_BAND_2200_7200 = 1,
    TX_H_BAND_D_BAND_AUTO_L = 2,
    TX_H_BAND_D_BAND_AUTO_H = 3,
};
enum tx_h_band_c_options {
    TX_H_BAND_C_BAND_400_3500 = 0,
    TX_H_BAND_C_BAND_2200_7200 = 1,
    TX_H_BAND_C_BAND_AUTO_L = 2,
    TX_H_BAND_C_BAND_AUTO_H = 3,
};
enum tx_h_band_b_options {
    TX_H_BAND_B_BAND_400_3500 = 0,
    TX_H_BAND_B_BAND_2200_7200 = 1,
    TX_H_BAND_B_BAND_AUTO_L = 2,
    TX_H_BAND_B_BAND_AUTO_H = 3,
};
enum tx_h_band_a_options {
    TX_H_BAND_A_BAND_400_3500 = 0,
    TX_H_BAND_A_BAND_2200_7200 = 1,
    TX_H_BAND_A_BAND_AUTO_L = 2,
    TX_H_BAND_A_BAND_AUTO_H = 3,
};

enum tx_h_band_fields_t {
    TX_H_BAND_D_OFF = 0xc,
    TX_H_BAND_D_MSK = 0x3000,
    TX_H_BAND_C_OFF = 0x8,
    TX_H_BAND_C_MSK = 0x300,
    TX_H_BAND_B_OFF = 0x4,
    TX_H_BAND_B_MSK = 0x30,
    TX_H_BAND_A_OFF = 0x0,
    TX_H_BAND_A_MSK = 0x3,
};
#define GET_M2_DSDR_USR_TX_H_BAND_D(x) (((x) & TX_H_BAND_D_MSK) >> TX_H_BAND_D_OFF)
#define GET_M2_DSDR_USR_TX_H_BAND_C(x) (((x) & TX_H_BAND_C_MSK) >> TX_H_BAND_C_OFF)
#define GET_M2_DSDR_USR_TX_H_BAND_B(x) (((x) & TX_H_BAND_B_MSK) >> TX_H_BAND_B_OFF)
#define GET_M2_DSDR_USR_TX_H_BAND_A(x) (((x) & TX_H_BAND_A_MSK) >> TX_H_BAND_A_OFF)
#define SET_M2_DSDR_USR_TX_H_BAND_D(p, f) (p) = ((p) & ~TX_H_BAND_D_MSK) | (((f) << TX_H_BAND_D_OFF) & TX_H_BAND_D_MSK)
#define SET_M2_DSDR_USR_TX_H_BAND_C(p, f) (p) = ((p) & ~TX_H_BAND_C_MSK) | (((f) << TX_H_BAND_C_OFF) & TX_H_BAND_C_MSK)
#define SET_M2_DSDR_USR_TX_H_BAND_B(p, f) (p) = ((p) & ~TX_H_BAND_B_MSK) | (((f) << TX_H_BAND_B_OFF) & TX_H_BAND_B_MSK)
#define SET_M2_DSDR_USR_TX_H_BAND_A(p, f) (p) = ((p) & ~TX_H_BAND_A_MSK) | (((f) << TX_H_BAND_A_OFF) & TX_H_BAND_A_MSK)

#define MAKE_M2_DSDR_USR_TX_H_BAND(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(TX_H_BAND, \
    (((d) << TX_H_BAND_D_OFF) & TX_H_BAND_D_MSK) |  \
    (((c) << TX_H_BAND_C_OFF) & TX_H_BAND_C_MSK) |  \
    (((b) << TX_H_BAND_B_OFF) & TX_H_BAND_B_MSK) |  \
    (((a) << TX_H_BAND_A_OFF) & TX_H_BAND_A_MSK))
// Register R69 [0x45] -- ANT_SEL
enum ant_sel_d_options {
    ANT_SEL_D_RX_TO_RX_AND_TX_TO_TRX = 0,
    ANT_SEL_D_RX_TO_TRX_AND_TX_TERM = 1,
    ANT_SEL_D_RX_TO_RX_AND_TX_TERM = 2,
    ANT_SEL_D_RX_TX_LOOPBACK = 3,
    ANT_SEL_D_TDD_DRIVEN_AUTO = 4,
};
enum ant_sel_c_options {
    ANT_SEL_C_RX_TO_RX_AND_TX_TO_TRX = 0,
    ANT_SEL_C_RX_TO_TRX_AND_TX_TERM = 1,
    ANT_SEL_C_RX_TO_RX_AND_TX_TERM = 2,
    ANT_SEL_C_RX_TX_LOOPBACK = 3,
    ANT_SEL_C_TDD_DRIVEN_AUTO = 4,
};
enum ant_sel_b_options {
    ANT_SEL_B_RX_TO_RX_AND_TX_TO_TRX = 0,
    ANT_SEL_B_RX_TO_TRX_AND_TX_TERM = 1,
    ANT_SEL_B_RX_TO_RX_AND_TX_TERM = 2,
    ANT_SEL_B_RX_TX_LOOPBACK = 3,
    ANT_SEL_B_TDD_DRIVEN_AUTO = 4,
};
enum ant_sel_a_options {
    ANT_SEL_A_RX_TO_RX_AND_TX_TO_TRX = 0,
    ANT_SEL_A_RX_TO_TRX_AND_TX_TERM = 1,
    ANT_SEL_A_RX_TO_RX_AND_TX_TERM = 2,
    ANT_SEL_A_RX_TX_LOOPBACK = 3,
    ANT_SEL_A_TDD_DRIVEN_AUTO = 4,
};

enum ant_sel_fields_t {
    ANT_SEL_D_OFF = 0xc,
    ANT_SEL_D_MSK = 0x7000,
    ANT_SEL_C_OFF = 0x8,
    ANT_SEL_C_MSK = 0x700,
    ANT_SEL_B_OFF = 0x4,
    ANT_SEL_B_MSK = 0x70,
    ANT_SEL_A_OFF = 0x0,
    ANT_SEL_A_MSK = 0x7,
};
#define GET_M2_DSDR_USR_ANT_SEL_D(x) (((x) & ANT_SEL_D_MSK) >> ANT_SEL_D_OFF)
#define GET_M2_DSDR_USR_ANT_SEL_C(x) (((x) & ANT_SEL_C_MSK) >> ANT_SEL_C_OFF)
#define GET_M2_DSDR_USR_ANT_SEL_B(x) (((x) & ANT_SEL_B_MSK) >> ANT_SEL_B_OFF)
#define GET_M2_DSDR_USR_ANT_SEL_A(x) (((x) & ANT_SEL_A_MSK) >> ANT_SEL_A_OFF)
#define SET_M2_DSDR_USR_ANT_SEL_D(p, f) (p) = ((p) & ~ANT_SEL_D_MSK) | (((f) << ANT_SEL_D_OFF) & ANT_SEL_D_MSK)
#define SET_M2_DSDR_USR_ANT_SEL_C(p, f) (p) = ((p) & ~ANT_SEL_C_MSK) | (((f) << ANT_SEL_C_OFF) & ANT_SEL_C_MSK)
#define SET_M2_DSDR_USR_ANT_SEL_B(p, f) (p) = ((p) & ~ANT_SEL_B_MSK) | (((f) << ANT_SEL_B_OFF) & ANT_SEL_B_MSK)
#define SET_M2_DSDR_USR_ANT_SEL_A(p, f) (p) = ((p) & ~ANT_SEL_A_MSK) | (((f) << ANT_SEL_A_OFF) & ANT_SEL_A_MSK)

#define MAKE_M2_DSDR_USR_ANT_SEL(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(ANT_SEL, \
    (((d) << ANT_SEL_D_OFF) & ANT_SEL_D_MSK) |  \
    (((c) << ANT_SEL_C_OFF) & ANT_SEL_C_MSK) |  \
    (((b) << ANT_SEL_B_OFF) & ANT_SEL_B_MSK) |  \
    (((a) << ANT_SEL_A_OFF) & ANT_SEL_A_MSK))
// Register R70 [0x46] -- RX_CHEN

enum rx_chen_fields_t {
    RX_CHEN_D_OFF = 0x3,
    RX_CHEN_D_MSK = 0x8,
    RX_CHEN_C_OFF = 0x2,
    RX_CHEN_C_MSK = 0x4,
    RX_CHEN_B_OFF = 0x1,
    RX_CHEN_B_MSK = 0x2,
    RX_CHEN_A_OFF = 0x0,
    RX_CHEN_A_MSK = 0x1,
};
#define GET_M2_DSDR_USR_RX_CHEN_D(x) (((x) & RX_CHEN_D_MSK) >> RX_CHEN_D_OFF)
#define GET_M2_DSDR_USR_RX_CHEN_C(x) (((x) & RX_CHEN_C_MSK) >> RX_CHEN_C_OFF)
#define GET_M2_DSDR_USR_RX_CHEN_B(x) (((x) & RX_CHEN_B_MSK) >> RX_CHEN_B_OFF)
#define GET_M2_DSDR_USR_RX_CHEN_A(x) (((x) & RX_CHEN_A_MSK) >> RX_CHEN_A_OFF)
#define SET_M2_DSDR_USR_RX_CHEN_D(p, f) (p) = ((p) & ~RX_CHEN_D_MSK) | (((f) << RX_CHEN_D_OFF) & RX_CHEN_D_MSK)
#define SET_M2_DSDR_USR_RX_CHEN_C(p, f) (p) = ((p) & ~RX_CHEN_C_MSK) | (((f) << RX_CHEN_C_OFF) & RX_CHEN_C_MSK)
#define SET_M2_DSDR_USR_RX_CHEN_B(p, f) (p) = ((p) & ~RX_CHEN_B_MSK) | (((f) << RX_CHEN_B_OFF) & RX_CHEN_B_MSK)
#define SET_M2_DSDR_USR_RX_CHEN_A(p, f) (p) = ((p) & ~RX_CHEN_A_MSK) | (((f) << RX_CHEN_A_OFF) & RX_CHEN_A_MSK)

#define MAKE_M2_DSDR_USR_RX_CHEN(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(RX_CHEN, \
    (((d) << RX_CHEN_D_OFF) & RX_CHEN_D_MSK) |  \
    (((c) << RX_CHEN_C_OFF) & RX_CHEN_C_MSK) |  \
    (((b) << RX_CHEN_B_OFF) & RX_CHEN_B_MSK) |  \
    (((a) << RX_CHEN_A_OFF) & RX_CHEN_A_MSK))
// Register R71 [0x47] -- TX_CHEN

enum tx_chen_fields_t {
    TX_CHEN_D_OFF = 0x3,
    TX_CHEN_D_MSK = 0x8,
    TX_CHEN_C_OFF = 0x2,
    TX_CHEN_C_MSK = 0x4,
    TX_CHEN_B_OFF = 0x1,
    TX_CHEN_B_MSK = 0x2,
    TX_CHEN_A_OFF = 0x0,
    TX_CHEN_A_MSK = 0x1,
};
#define GET_M2_DSDR_USR_TX_CHEN_D(x) (((x) & TX_CHEN_D_MSK) >> TX_CHEN_D_OFF)
#define GET_M2_DSDR_USR_TX_CHEN_C(x) (((x) & TX_CHEN_C_MSK) >> TX_CHEN_C_OFF)
#define GET_M2_DSDR_USR_TX_CHEN_B(x) (((x) & TX_CHEN_B_MSK) >> TX_CHEN_B_OFF)
#define GET_M2_DSDR_USR_TX_CHEN_A(x) (((x) & TX_CHEN_A_MSK) >> TX_CHEN_A_OFF)
#define SET_M2_DSDR_USR_TX_CHEN_D(p, f) (p) = ((p) & ~TX_CHEN_D_MSK) | (((f) << TX_CHEN_D_OFF) & TX_CHEN_D_MSK)
#define SET_M2_DSDR_USR_TX_CHEN_C(p, f) (p) = ((p) & ~TX_CHEN_C_MSK) | (((f) << TX_CHEN_C_OFF) & TX_CHEN_C_MSK)
#define SET_M2_DSDR_USR_TX_CHEN_B(p, f) (p) = ((p) & ~TX_CHEN_B_MSK) | (((f) << TX_CHEN_B_OFF) & TX_CHEN_B_MSK)
#define SET_M2_DSDR_USR_TX_CHEN_A(p, f) (p) = ((p) & ~TX_CHEN_A_MSK) | (((f) << TX_CHEN_A_OFF) & TX_CHEN_A_MSK)

#define MAKE_M2_DSDR_USR_TX_CHEN(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(TX_CHEN, \
    (((d) << TX_CHEN_D_OFF) & TX_CHEN_D_MSK) |  \
    (((c) << TX_CHEN_C_OFF) & TX_CHEN_C_MSK) |  \
    (((b) << TX_CHEN_B_OFF) & TX_CHEN_B_MSK) |  \
    (((a) << TX_CHEN_A_OFF) & TX_CHEN_A_MSK))
// Register R72 [0x48] -- RX_8KA_LNA

enum rx_8ka_lna_fields_t {
    RX_8KA_LNA_D_OFF = 0x12,
    RX_8KA_LNA_D_MSK = 0x7c0000,
    RX_8KA_LNA_C_OFF = 0xc,
    RX_8KA_LNA_C_MSK = 0x1f000,
    RX_8KA_LNA_B_OFF = 0x6,
    RX_8KA_LNA_B_MSK = 0x7c0,
    RX_8KA_LNA_A_OFF = 0x0,
    RX_8KA_LNA_A_MSK = 0x1f,
};
#define GET_M2_DSDR_USR_RX_8KA_LNA_D(x) (((x) & RX_8KA_LNA_D_MSK) >> RX_8KA_LNA_D_OFF)
#define GET_M2_DSDR_USR_RX_8KA_LNA_C(x) (((x) & RX_8KA_LNA_C_MSK) >> RX_8KA_LNA_C_OFF)
#define GET_M2_DSDR_USR_RX_8KA_LNA_B(x) (((x) & RX_8KA_LNA_B_MSK) >> RX_8KA_LNA_B_OFF)
#define GET_M2_DSDR_USR_RX_8KA_LNA_A(x) (((x) & RX_8KA_LNA_A_MSK) >> RX_8KA_LNA_A_OFF)
#define SET_M2_DSDR_USR_RX_8KA_LNA_D(p, f) (p) = ((p) & ~RX_8KA_LNA_D_MSK) | (((f) << RX_8KA_LNA_D_OFF) & RX_8KA_LNA_D_MSK)
#define SET_M2_DSDR_USR_RX_8KA_LNA_C(p, f) (p) = ((p) & ~RX_8KA_LNA_C_MSK) | (((f) << RX_8KA_LNA_C_OFF) & RX_8KA_LNA_C_MSK)
#define SET_M2_DSDR_USR_RX_8KA_LNA_B(p, f) (p) = ((p) & ~RX_8KA_LNA_B_MSK) | (((f) << RX_8KA_LNA_B_OFF) & RX_8KA_LNA_B_MSK)
#define SET_M2_DSDR_USR_RX_8KA_LNA_A(p, f) (p) = ((p) & ~RX_8KA_LNA_A_MSK) | (((f) << RX_8KA_LNA_A_OFF) & RX_8KA_LNA_A_MSK)

#define MAKE_M2_DSDR_USR_RX_8KA_LNA(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(RX_8KA_LNA, \
    (((d) << RX_8KA_LNA_D_OFF) & RX_8KA_LNA_D_MSK) |  \
    (((c) << RX_8KA_LNA_C_OFF) & RX_8KA_LNA_C_MSK) |  \
    (((b) << RX_8KA_LNA_B_OFF) & RX_8KA_LNA_B_MSK) |  \
    (((a) << RX_8KA_LNA_A_OFF) & RX_8KA_LNA_A_MSK))
// Register R73 [0x49] -- RX_8KA_PA

enum rx_8ka_pa_fields_t {
    RX_8KA_PA_D_OFF = 0x12,
    RX_8KA_PA_D_MSK = 0x7c0000,
    RX_8KA_PA_C_OFF = 0xc,
    RX_8KA_PA_C_MSK = 0x1f000,
    RX_8KA_PA_B_OFF = 0x6,
    RX_8KA_PA_B_MSK = 0x7c0,
    RX_8KA_PA_A_OFF = 0x0,
    RX_8KA_PA_A_MSK = 0x1f,
};
#define GET_M2_DSDR_USR_RX_8KA_PA_D(x) (((x) & RX_8KA_PA_D_MSK) >> RX_8KA_PA_D_OFF)
#define GET_M2_DSDR_USR_RX_8KA_PA_C(x) (((x) & RX_8KA_PA_C_MSK) >> RX_8KA_PA_C_OFF)
#define GET_M2_DSDR_USR_RX_8KA_PA_B(x) (((x) & RX_8KA_PA_B_MSK) >> RX_8KA_PA_B_OFF)
#define GET_M2_DSDR_USR_RX_8KA_PA_A(x) (((x) & RX_8KA_PA_A_MSK) >> RX_8KA_PA_A_OFF)
#define SET_M2_DSDR_USR_RX_8KA_PA_D(p, f) (p) = ((p) & ~RX_8KA_PA_D_MSK) | (((f) << RX_8KA_PA_D_OFF) & RX_8KA_PA_D_MSK)
#define SET_M2_DSDR_USR_RX_8KA_PA_C(p, f) (p) = ((p) & ~RX_8KA_PA_C_MSK) | (((f) << RX_8KA_PA_C_OFF) & RX_8KA_PA_C_MSK)
#define SET_M2_DSDR_USR_RX_8KA_PA_B(p, f) (p) = ((p) & ~RX_8KA_PA_B_MSK) | (((f) << RX_8KA_PA_B_OFF) & RX_8KA_PA_B_MSK)
#define SET_M2_DSDR_USR_RX_8KA_PA_A(p, f) (p) = ((p) & ~RX_8KA_PA_A_MSK) | (((f) << RX_8KA_PA_A_OFF) & RX_8KA_PA_A_MSK)

#define MAKE_M2_DSDR_USR_RX_8KA_PA(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(RX_8KA_PA, \
    (((d) << RX_8KA_PA_D_OFF) & RX_8KA_PA_D_MSK) |  \
    (((c) << RX_8KA_PA_C_OFF) & RX_8KA_PA_C_MSK) |  \
    (((b) << RX_8KA_PA_B_OFF) & RX_8KA_PA_B_MSK) |  \
    (((a) << RX_8KA_PA_A_OFF) & RX_8KA_PA_A_MSK))
// Register R74 [0x4a] -- RX_8KB

enum rx_8kb_fields_t {
    RX_8KB_D_OFF = 0xc,
    RX_8KB_D_MSK = 0xf000,
    RX_8KB_C_OFF = 0x8,
    RX_8KB_C_MSK = 0xf00,
    RX_8KB_B_OFF = 0x4,
    RX_8KB_B_MSK = 0xf0,
    RX_8KB_A_OFF = 0x0,
    RX_8KB_A_MSK = 0xf,
};
#define GET_M2_DSDR_USR_RX_8KB_D(x) (((x) & RX_8KB_D_MSK) >> RX_8KB_D_OFF)
#define GET_M2_DSDR_USR_RX_8KB_C(x) (((x) & RX_8KB_C_MSK) >> RX_8KB_C_OFF)
#define GET_M2_DSDR_USR_RX_8KB_B(x) (((x) & RX_8KB_B_MSK) >> RX_8KB_B_OFF)
#define GET_M2_DSDR_USR_RX_8KB_A(x) (((x) & RX_8KB_A_MSK) >> RX_8KB_A_OFF)
#define SET_M2_DSDR_USR_RX_8KB_D(p, f) (p) = ((p) & ~RX_8KB_D_MSK) | (((f) << RX_8KB_D_OFF) & RX_8KB_D_MSK)
#define SET_M2_DSDR_USR_RX_8KB_C(p, f) (p) = ((p) & ~RX_8KB_C_MSK) | (((f) << RX_8KB_C_OFF) & RX_8KB_C_MSK)
#define SET_M2_DSDR_USR_RX_8KB_B(p, f) (p) = ((p) & ~RX_8KB_B_MSK) | (((f) << RX_8KB_B_OFF) & RX_8KB_B_MSK)
#define SET_M2_DSDR_USR_RX_8KB_A(p, f) (p) = ((p) & ~RX_8KB_A_MSK) | (((f) << RX_8KB_A_OFF) & RX_8KB_A_MSK)

#define MAKE_M2_DSDR_USR_RX_8KB(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(RX_8KB, \
    (((d) << RX_8KB_D_OFF) & RX_8KB_D_MSK) |  \
    (((c) << RX_8KB_C_OFF) & RX_8KB_C_MSK) |  \
    (((b) << RX_8KB_B_OFF) & RX_8KB_B_MSK) |  \
    (((a) << RX_8KB_A_OFF) & RX_8KB_A_MSK))
// Register R75 [0x4b] -- TX_8KB

enum tx_8kb_fields_t {
    TX_8KB_D_OFF = 0xc,
    TX_8KB_D_MSK = 0xf000,
    TX_8KB_C_OFF = 0x8,
    TX_8KB_C_MSK = 0xf00,
    TX_8KB_B_OFF = 0x4,
    TX_8KB_B_MSK = 0xf0,
    TX_8KB_A_OFF = 0x0,
    TX_8KB_A_MSK = 0xf,
};
#define GET_M2_DSDR_USR_TX_8KB_D(x) (((x) & TX_8KB_D_MSK) >> TX_8KB_D_OFF)
#define GET_M2_DSDR_USR_TX_8KB_C(x) (((x) & TX_8KB_C_MSK) >> TX_8KB_C_OFF)
#define GET_M2_DSDR_USR_TX_8KB_B(x) (((x) & TX_8KB_B_MSK) >> TX_8KB_B_OFF)
#define GET_M2_DSDR_USR_TX_8KB_A(x) (((x) & TX_8KB_A_MSK) >> TX_8KB_A_OFF)
#define SET_M2_DSDR_USR_TX_8KB_D(p, f) (p) = ((p) & ~TX_8KB_D_MSK) | (((f) << TX_8KB_D_OFF) & TX_8KB_D_MSK)
#define SET_M2_DSDR_USR_TX_8KB_C(p, f) (p) = ((p) & ~TX_8KB_C_MSK) | (((f) << TX_8KB_C_OFF) & TX_8KB_C_MSK)
#define SET_M2_DSDR_USR_TX_8KB_B(p, f) (p) = ((p) & ~TX_8KB_B_MSK) | (((f) << TX_8KB_B_OFF) & TX_8KB_B_MSK)
#define SET_M2_DSDR_USR_TX_8KB_A(p, f) (p) = ((p) & ~TX_8KB_A_MSK) | (((f) << TX_8KB_A_OFF) & TX_8KB_A_MSK)

#define MAKE_M2_DSDR_USR_TX_8KB(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(TX_8KB, \
    (((d) << TX_8KB_D_OFF) & TX_8KB_D_MSK) |  \
    (((c) << TX_8KB_C_OFF) & TX_8KB_C_MSK) |  \
    (((b) << TX_8KB_B_OFF) & TX_8KB_B_MSK) |  \
    (((a) << TX_8KB_A_OFF) & TX_8KB_A_MSK))
// Register R76 [0x4c] -- PA_2ND_BP

enum pa_2nd_bp_fields_t {
    PA_2ND_BP_D_OFF = 0x3,
    PA_2ND_BP_D_MSK = 0x8,
    PA_2ND_BP_C_OFF = 0x2,
    PA_2ND_BP_C_MSK = 0x4,
    PA_2ND_BP_B_OFF = 0x1,
    PA_2ND_BP_B_MSK = 0x2,
    PA_2ND_BP_A_OFF = 0x0,
    PA_2ND_BP_A_MSK = 0x1,
};
#define GET_M2_DSDR_USR_PA_2ND_BP_D(x) (((x) & PA_2ND_BP_D_MSK) >> PA_2ND_BP_D_OFF)
#define GET_M2_DSDR_USR_PA_2ND_BP_C(x) (((x) & PA_2ND_BP_C_MSK) >> PA_2ND_BP_C_OFF)
#define GET_M2_DSDR_USR_PA_2ND_BP_B(x) (((x) & PA_2ND_BP_B_MSK) >> PA_2ND_BP_B_OFF)
#define GET_M2_DSDR_USR_PA_2ND_BP_A(x) (((x) & PA_2ND_BP_A_MSK) >> PA_2ND_BP_A_OFF)
#define SET_M2_DSDR_USR_PA_2ND_BP_D(p, f) (p) = ((p) & ~PA_2ND_BP_D_MSK) | (((f) << PA_2ND_BP_D_OFF) & PA_2ND_BP_D_MSK)
#define SET_M2_DSDR_USR_PA_2ND_BP_C(p, f) (p) = ((p) & ~PA_2ND_BP_C_MSK) | (((f) << PA_2ND_BP_C_OFF) & PA_2ND_BP_C_MSK)
#define SET_M2_DSDR_USR_PA_2ND_BP_B(p, f) (p) = ((p) & ~PA_2ND_BP_B_MSK) | (((f) << PA_2ND_BP_B_OFF) & PA_2ND_BP_B_MSK)
#define SET_M2_DSDR_USR_PA_2ND_BP_A(p, f) (p) = ((p) & ~PA_2ND_BP_A_MSK) | (((f) << PA_2ND_BP_A_OFF) & PA_2ND_BP_A_MSK)

#define MAKE_M2_DSDR_USR_PA_2ND_BP(d, c, b, a) MAKE_M2_DSDR_USR_REG_WR(PA_2ND_BP, \
    (((d) << PA_2ND_BP_D_OFF) & PA_2ND_BP_D_MSK) |  \
    (((c) << PA_2ND_BP_C_OFF) & PA_2ND_BP_C_MSK) |  \
    (((b) << PA_2ND_BP_B_OFF) & PA_2ND_BP_B_MSK) |  \
    (((a) << PA_2ND_BP_A_OFF) & PA_2ND_BP_A_MSK))
