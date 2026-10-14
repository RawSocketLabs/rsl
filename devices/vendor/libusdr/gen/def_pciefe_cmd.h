enum pciefe_cmd_regs_t {
    FECMD_DAC = 0x0,
    FECMD_TXSEL = 0x1,
    FECMD_RXSEL = 0x2,
    FECMD_DUPLSEL = 0x3,
    FECMD_LOOPBACK = 0x4,
    FECMD_LED = 0x5,
    FECMD_ATTN = 0x6,
    FECMD_CTRL_LNA = 0x7,
    FECMD_CTRL_PA = 0x8,
    FECMD_GPS = 0x9,
    FECMD_OSC = 0xa,
};
#define MAKE_PCIEFE_CMD_REG_WR(a, v) (((a) << 16) | ((v) & 0xffff))
#define MAKE_PCIEFE_CMD_REG_RD(a) (((a) << 16))
// Register R0 [0x0] -- FECMD_DAC

enum fecmd_dac_fields_t {
    VOUT_OFF = 0x0,
    VOUT_MSK = 0xffff,
};
#define MAKE_PCIEFE_CMD_FECMD_DAC(vout) MAKE_PCIEFE_CMD_REG_WR(FECMD_DAC, \
    (((vout) << VOUT_OFF) & VOUT_MSK))
// Register R1 [0x1] -- FECMD_TXSEL
enum txsel_options {
    TXSEL_TX_LPF400 = 0,
    TXSEL_TX_LPF1200 = 1,
    TXSEL_TX_LPF2100 = 2,
    TXSEL_TX_BYPASS = 3,
};

enum fecmd_txsel_fields_t {
    TXSEL_OFF = 0x0,
    TXSEL_MSK = 0x3,
};
#define MAKE_PCIEFE_CMD_FECMD_TXSEL(txsel) MAKE_PCIEFE_CMD_REG_WR(FECMD_TXSEL, \
    (((txsel) << TXSEL_OFF) & TXSEL_MSK))
// Register R2 [0x2] -- FECMD_RXSEL
enum rxsel_options {
    RXSEL_RX_LPF1200 = 0,
    RXSEL_RX_LPF2100 = 1,
    RXSEL_RX_BPF2100_3000 = 2,
    RXSEL_RX_BPF3000_4200 = 3,
};

enum fecmd_rxsel_fields_t {
    RXSEL_OFF = 0x0,
    RXSEL_MSK = 0x3,
};
#define MAKE_PCIEFE_CMD_FECMD_RXSEL(rxsel) MAKE_PCIEFE_CMD_REG_WR(FECMD_RXSEL, \
    (((rxsel) << RXSEL_OFF) & RXSEL_MSK))
// Register R3 [0x3] -- FECMD_DUPLSEL
enum duplsel_options {
    DUPLSEL_TRX_BYPASS = 0,
    DUPLSEL_TRX_BAND2 = 1,
    DUPLSEL_TRX_BAND3 = 2,
    DUPLSEL_TRX_BAND5 = 3,
    DUPLSEL_TRX_BAND7 = 4,
    DUPLSEL_TRX_BAND8 = 5,
};

enum fecmd_duplsel_fields_t {
    DUPLSEL_OFF = 0x0,
    DUPLSEL_MSK = 0x7,
};
#define MAKE_PCIEFE_CMD_FECMD_DUPLSEL(duplsel) MAKE_PCIEFE_CMD_REG_WR(FECMD_DUPLSEL, \
    (((duplsel) << DUPLSEL_OFF) & DUPLSEL_MSK))
// Register R4 [0x4] -- FECMD_LOOPBACK

enum fecmd_loopback_fields_t {
    TRX_OFF = 0x0,
    TRX_MSK = 0x1,
};
#define MAKE_PCIEFE_CMD_FECMD_LOOPBACK(trx) MAKE_PCIEFE_CMD_REG_WR(FECMD_LOOPBACK, \
    (((trx) << TRX_OFF) & TRX_MSK))
// Register R5 [0x5] -- FECMD_LED

enum fecmd_led_fields_t {
    LED4_OFF = 0x3,
    LED4_MSK = 0x8,
    LED3_OFF = 0x2,
    LED3_MSK = 0x4,
    LED2_OFF = 0x1,
    LED2_MSK = 0x2,
    LED1_OFF = 0x0,
    LED1_MSK = 0x1,
};
#define MAKE_PCIEFE_CMD_FECMD_LED(led4, led3, led2, led1) MAKE_PCIEFE_CMD_REG_WR(FECMD_LED, \
    (((led4) << LED4_OFF) & LED4_MSK) |  \
    (((led3) << LED3_OFF) & LED3_MSK) |  \
    (((led2) << LED2_OFF) & LED2_MSK) |  \
    (((led1) << LED1_OFF) & LED1_MSK))
// Register R6 [0x6] -- FECMD_ATTN
enum val_options {
    VAL_IL = 0,
    VAL_6DB = 1,
    VAL_12DB = 2,
    VAL_18DB = 3,
};

enum fecmd_attn_fields_t {
    VAL_OFF = 0x0,
    VAL_MSK = 0x3,
};
#define MAKE_PCIEFE_CMD_FECMD_ATTN(val) MAKE_PCIEFE_CMD_REG_WR(FECMD_ATTN, \
    (((val) << VAL_OFF) & VAL_MSK))
// Register R7 [0x7] -- FECMD_CTRL_LNA

enum fecmd_ctrl_lna_fields_t {
    LNA_OFF = 0x0,
    LNA_MSK = 0x1,
};
#define MAKE_PCIEFE_CMD_FECMD_CTRL_LNA(lna) MAKE_PCIEFE_CMD_REG_WR(FECMD_CTRL_LNA, \
    (((lna) << LNA_OFF) & LNA_MSK))
// Register R8 [0x8] -- FECMD_CTRL_PA

enum fecmd_ctrl_pa_fields_t {
    PA_OFF = 0x0,
    PA_MSK = 0x1,
};
#define MAKE_PCIEFE_CMD_FECMD_CTRL_PA(pa) MAKE_PCIEFE_CMD_REG_WR(FECMD_CTRL_PA, \
    (((pa) << PA_OFF) & PA_MSK))
// Register R9 [0x9] -- FECMD_GPS

enum fecmd_gps_fields_t {
    EN_GPS_OFF = 0x0,
    EN_GPS_MSK = 0x1,
};
#define MAKE_PCIEFE_CMD_FECMD_GPS(en_gps) MAKE_PCIEFE_CMD_REG_WR(FECMD_GPS, \
    (((en_gps) << EN_GPS_OFF) & EN_GPS_MSK))
// Register R10 [0xa] -- FECMD_OSC

enum fecmd_osc_fields_t {
    EN_OSC_OFF = 0x0,
    EN_OSC_MSK = 0x1,
};
#define MAKE_PCIEFE_CMD_FECMD_OSC(en_osc) MAKE_PCIEFE_CMD_REG_WR(FECMD_OSC, \
    (((en_osc) << EN_OSC_OFF) & EN_OSC_MSK))
