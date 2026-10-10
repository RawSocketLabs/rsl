enum ext_xmass_ctrl_regs_t {
    P0 = 0x0,
    P1 = 0x1,
};
#define MAKE_EXT_XMASS_CTRL_REG_WR(a, v) (0x80000000 | ((a) << 24) | ((v) & 0xffffff))
#define MAKE_EXT_XMASS_CTRL_REG_RD(a) (((a) << 24))
// Register R0 [0x0] -- P0

enum p0_fields_t {
    P0_EN_LMX_OFF = 0x7,
    P0_EN_LMX_MSK = 0x80,
    P0_SYSREF_1PPS_SEL_OFF = 0x6,
    P0_SYSREF_1PPS_SEL_MSK = 0x40,
    P0_LMK_SYNCN_OFF = 0x5,
    P0_LMK_SYNCN_MSK = 0x20,
    P0_GPS_PWREN_OFF = 0x4,
    P0_GPS_PWREN_MSK = 0x10,
    P0_RF_CAL_SRC_SEL_OFF = 0x3,
    P0_RF_CAL_SRC_SEL_MSK = 0x8,
    P0_RF_CAL_DST_SEL_OFF = 0x2,
    P0_RF_CAL_DST_SEL_MSK = 0x4,
    P0_BLOCAL_OFF = 0x1,
    P0_BLOCAL_MSK = 0x2,
    P0_BDISTRIB_OFF = 0x0,
    P0_BDISTRIB_MSK = 0x1,
};
#define GET_EXT_XMASS_CTRL_P0_EN_LMX(x) (((x) & P0_EN_LMX_MSK) >> P0_EN_LMX_OFF)
#define GET_EXT_XMASS_CTRL_P0_SYSREF_1PPS_SEL(x) (((x) & P0_SYSREF_1PPS_SEL_MSK) >> P0_SYSREF_1PPS_SEL_OFF)
#define GET_EXT_XMASS_CTRL_P0_LMK_SYNCN(x) (((x) & P0_LMK_SYNCN_MSK) >> P0_LMK_SYNCN_OFF)
#define GET_EXT_XMASS_CTRL_P0_GPS_PWREN(x) (((x) & P0_GPS_PWREN_MSK) >> P0_GPS_PWREN_OFF)
#define GET_EXT_XMASS_CTRL_P0_RF_CAL_SRC_SEL(x) (((x) & P0_RF_CAL_SRC_SEL_MSK) >> P0_RF_CAL_SRC_SEL_OFF)
#define GET_EXT_XMASS_CTRL_P0_RF_CAL_DST_SEL(x) (((x) & P0_RF_CAL_DST_SEL_MSK) >> P0_RF_CAL_DST_SEL_OFF)
#define GET_EXT_XMASS_CTRL_P0_BLOCAL(x) (((x) & P0_BLOCAL_MSK) >> P0_BLOCAL_OFF)
#define GET_EXT_XMASS_CTRL_P0_BDISTRIB(x) (((x) & P0_BDISTRIB_MSK) >> P0_BDISTRIB_OFF)
#define SET_EXT_XMASS_CTRL_P0_EN_LMX(p, f) (p) = ((p) & ~P0_EN_LMX_MSK) | (((f) << P0_EN_LMX_OFF) & P0_EN_LMX_MSK)
#define SET_EXT_XMASS_CTRL_P0_SYSREF_1PPS_SEL(p, f) (p) = ((p) & ~P0_SYSREF_1PPS_SEL_MSK) | (((f) << P0_SYSREF_1PPS_SEL_OFF) & P0_SYSREF_1PPS_SEL_MSK)
#define SET_EXT_XMASS_CTRL_P0_LMK_SYNCN(p, f) (p) = ((p) & ~P0_LMK_SYNCN_MSK) | (((f) << P0_LMK_SYNCN_OFF) & P0_LMK_SYNCN_MSK)
#define SET_EXT_XMASS_CTRL_P0_GPS_PWREN(p, f) (p) = ((p) & ~P0_GPS_PWREN_MSK) | (((f) << P0_GPS_PWREN_OFF) & P0_GPS_PWREN_MSK)
#define SET_EXT_XMASS_CTRL_P0_RF_CAL_SRC_SEL(p, f) (p) = ((p) & ~P0_RF_CAL_SRC_SEL_MSK) | (((f) << P0_RF_CAL_SRC_SEL_OFF) & P0_RF_CAL_SRC_SEL_MSK)
#define SET_EXT_XMASS_CTRL_P0_RF_CAL_DST_SEL(p, f) (p) = ((p) & ~P0_RF_CAL_DST_SEL_MSK) | (((f) << P0_RF_CAL_DST_SEL_OFF) & P0_RF_CAL_DST_SEL_MSK)
#define SET_EXT_XMASS_CTRL_P0_BLOCAL(p, f) (p) = ((p) & ~P0_BLOCAL_MSK) | (((f) << P0_BLOCAL_OFF) & P0_BLOCAL_MSK)
#define SET_EXT_XMASS_CTRL_P0_BDISTRIB(p, f) (p) = ((p) & ~P0_BDISTRIB_MSK) | (((f) << P0_BDISTRIB_OFF) & P0_BDISTRIB_MSK)

#define MAKE_EXT_XMASS_CTRL_P0(en_lmx, sysref_1pps_sel, lmk_syncn, gps_pwren, rf_cal_src_sel, rf_cal_dst_sel, blocal, bdistrib) MAKE_EXT_XMASS_CTRL_REG_WR(P0, \
    (((en_lmx) << P0_EN_LMX_OFF) & P0_EN_LMX_MSK) |  \
    (((sysref_1pps_sel) << P0_SYSREF_1PPS_SEL_OFF) & P0_SYSREF_1PPS_SEL_MSK) |  \
    (((lmk_syncn) << P0_LMK_SYNCN_OFF) & P0_LMK_SYNCN_MSK) |  \
    (((gps_pwren) << P0_GPS_PWREN_OFF) & P0_GPS_PWREN_MSK) |  \
    (((rf_cal_src_sel) << P0_RF_CAL_SRC_SEL_OFF) & P0_RF_CAL_SRC_SEL_MSK) |  \
    (((rf_cal_dst_sel) << P0_RF_CAL_DST_SEL_OFF) & P0_RF_CAL_DST_SEL_MSK) |  \
    (((blocal) << P0_BLOCAL_OFF) & P0_BLOCAL_MSK) |  \
    (((bdistrib) << P0_BDISTRIB_OFF) & P0_BDISTRIB_MSK))
// Register R1 [0x1] -- P1

enum p1_fields_t {
    P1_RTS_OFF = 0x5,
    P1_RTS_MSK = 0xe0,
    P1_SYSREF_GPSRX_SEL_OFF = 0x4,
    P1_SYSREF_GPSRX_SEL_MSK = 0x10,
    P1_RF_NOISE_EN_OFF = 0x3,
    P1_RF_NOISE_EN_MSK = 0x8,
    P1_RF_LB_SW_OFF = 0x2,
    P1_RF_LB_SW_MSK = 0x4,
    P1_RF_CAL_SW_OFF = 0x1,
    P1_RF_CAL_SW_MSK = 0x2,
    P1_RF_EN_OFF = 0x0,
    P1_RF_EN_MSK = 0x1,
};
#define GET_EXT_XMASS_CTRL_P1_RTS(x) (((x) & P1_RTS_MSK) >> P1_RTS_OFF)
#define GET_EXT_XMASS_CTRL_P1_SYSREF_GPSRX_SEL(x) (((x) & P1_SYSREF_GPSRX_SEL_MSK) >> P1_SYSREF_GPSRX_SEL_OFF)
#define GET_EXT_XMASS_CTRL_P1_RF_NOISE_EN(x) (((x) & P1_RF_NOISE_EN_MSK) >> P1_RF_NOISE_EN_OFF)
#define GET_EXT_XMASS_CTRL_P1_RF_LB_SW(x) (((x) & P1_RF_LB_SW_MSK) >> P1_RF_LB_SW_OFF)
#define GET_EXT_XMASS_CTRL_P1_RF_CAL_SW(x) (((x) & P1_RF_CAL_SW_MSK) >> P1_RF_CAL_SW_OFF)
#define GET_EXT_XMASS_CTRL_P1_RF_EN(x) (((x) & P1_RF_EN_MSK) >> P1_RF_EN_OFF)
#define SET_EXT_XMASS_CTRL_P1_RTS(p, f) (p) = ((p) & ~P1_RTS_MSK) | (((f) << P1_RTS_OFF) & P1_RTS_MSK)
#define SET_EXT_XMASS_CTRL_P1_SYSREF_GPSRX_SEL(p, f) (p) = ((p) & ~P1_SYSREF_GPSRX_SEL_MSK) | (((f) << P1_SYSREF_GPSRX_SEL_OFF) & P1_SYSREF_GPSRX_SEL_MSK)
#define SET_EXT_XMASS_CTRL_P1_RF_NOISE_EN(p, f) (p) = ((p) & ~P1_RF_NOISE_EN_MSK) | (((f) << P1_RF_NOISE_EN_OFF) & P1_RF_NOISE_EN_MSK)
#define SET_EXT_XMASS_CTRL_P1_RF_LB_SW(p, f) (p) = ((p) & ~P1_RF_LB_SW_MSK) | (((f) << P1_RF_LB_SW_OFF) & P1_RF_LB_SW_MSK)
#define SET_EXT_XMASS_CTRL_P1_RF_CAL_SW(p, f) (p) = ((p) & ~P1_RF_CAL_SW_MSK) | (((f) << P1_RF_CAL_SW_OFF) & P1_RF_CAL_SW_MSK)
#define SET_EXT_XMASS_CTRL_P1_RF_EN(p, f) (p) = ((p) & ~P1_RF_EN_MSK) | (((f) << P1_RF_EN_OFF) & P1_RF_EN_MSK)

#define MAKE_EXT_XMASS_CTRL_P1(rts, sysref_gpsrx_sel, rf_noise_en, rf_lb_sw, rf_cal_sw, rf_en) MAKE_EXT_XMASS_CTRL_REG_WR(P1, \
    (((rts) << P1_RTS_OFF) & P1_RTS_MSK) |  \
    (((sysref_gpsrx_sel) << P1_SYSREF_GPSRX_SEL_OFF) & P1_SYSREF_GPSRX_SEL_MSK) |  \
    (((rf_noise_en) << P1_RF_NOISE_EN_OFF) & P1_RF_NOISE_EN_MSK) |  \
    (((rf_lb_sw) << P1_RF_LB_SW_OFF) & P1_RF_LB_SW_MSK) |  \
    (((rf_cal_sw) << P1_RF_CAL_SW_OFF) & P1_RF_CAL_SW_MSK) |  \
    (((rf_en) << P1_RF_EN_OFF) & P1_RF_EN_MSK))
