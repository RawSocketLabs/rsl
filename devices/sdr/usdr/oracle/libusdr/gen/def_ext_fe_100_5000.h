enum ext_fe_100_5000_regs_t {
    FE_FREQ = 0x0,
    FE_ATTN = 0x1,
    FE_PRESEL = 0x2,
};
#define MAKE_EXT_FE_100_5000_REG_WR(a, v) (((a) << 24) | ((v) & 0xffffff))
#define MAKE_EXT_FE_100_5000_REG_RD(a) (((a) << 24))
// Register R0 [0x0] -- FE_FREQ

enum fe_freq_fields_t {
    KHZ_OFF = 0x0,
    KHZ_MSK = 0xffffff,
};
#define MAKE_EXT_FE_100_5000_FE_FREQ(khz) MAKE_EXT_FE_100_5000_REG_WR(FE_FREQ, \
    (((khz) << KHZ_OFF) & KHZ_MSK))
// Register R1 [0x1] -- FE_ATTN

enum fe_attn_fields_t {
    DB_OFF = 0x0,
    DB_MSK = 0x7f,
};
#define MAKE_EXT_FE_100_5000_FE_ATTN(db) MAKE_EXT_FE_100_5000_REG_WR(FE_ATTN, \
    (((db) << DB_OFF) & DB_MSK))
// Register R2 [0x2] -- FE_PRESEL

enum fe_presel_fields_t {
    BAND_OFF = 0x0,
    BAND_MSK = 0x3,
};
#define MAKE_EXT_FE_100_5000_FE_PRESEL(band) MAKE_EXT_FE_100_5000_REG_WR(FE_PRESEL, \
    (((band) << BAND_OFF) & BAND_MSK))
