enum adf4002b_regs_t {
    REG0 = 0x0,
    REG1 = 0x1,
    REG2 = 0x2,
    REG3 = 0x3,
};
#define MAKE_ADF4002B_REG_WR(a, v) (0x80000000 | ((a) << 24) | ((v) & 0xffffff))
#define MAKE_ADF4002B_REG_RD(a) (((a) << 24))
enum adf4002b_muxcontrol_opts_t {
    MUXCONTROL_OPTS_Tri_State = 0x0,
    MUXCONTROL_OPTS_Dig_LD = 0x1,
    MUXCONTROL_OPTS_N_Div_Out = 0x2,
    MUXCONTROL_OPTS_DVdd = 0x3,
    MUXCONTROL_OPTS_R_Div_Out = 0x4,
    MUXCONTROL_OPTS_OpenD_LD = 0x5,
    MUXCONTROL_OPTS_SDO = 0x6,
    MUXCONTROL_OPTS_DGND = 0x7,
};
// Register R0 [0x0] -- REG0
enum antib_pw_options {
    ANTIB_PW_2_9NS = 0,
    ANTIB_PW_NA = 1,
    ANTIB_PW_6_0NS = 2,
    ANTIB_PW_2_9NS = 3,
};

enum reg0_fields_t {
    LOCK_DET_PREC_OFF = 0x14,
    LOCK_DET_PREC_MSK = 0x100000,
    ANTIB_PW_OFF = 0x10,
    ANTIB_PW_MSK = 0x30000,
    R_OFF = 0x2,
    R_MSK = 0xfffc,
};
#define GET_ADF4002B_LOCK_DET_PREC(x) (((x) & LOCK_DET_PREC_MSK) >> LOCK_DET_PREC_OFF)
#define GET_ADF4002B_ANTIB_PW(x) (((x) & ANTIB_PW_MSK) >> ANTIB_PW_OFF)
#define GET_ADF4002B_R(x) (((x) & R_MSK) >> R_OFF)
#define SET_ADF4002B_LOCK_DET_PREC(p, f) (p) = ((p) & ~LOCK_DET_PREC_MSK) | (((f) << LOCK_DET_PREC_OFF) & LOCK_DET_PREC_MSK)
#define SET_ADF4002B_ANTIB_PW(p, f) (p) = ((p) & ~ANTIB_PW_MSK) | (((f) << ANTIB_PW_OFF) & ANTIB_PW_MSK)
#define SET_ADF4002B_R(p, f) (p) = ((p) & ~R_MSK) | (((f) << R_OFF) & R_MSK)

#define MAKE_ADF4002B_REG0(lock_det_prec, antib_pw, r) MAKE_ADF4002B_REG_WR(REG0, \
    (((lock_det_prec) << LOCK_DET_PREC_OFF) & LOCK_DET_PREC_MSK) |  \
    (((antib_pw) << ANTIB_PW_OFF) & ANTIB_PW_MSK) |  \
    (((r) << R_OFF) & R_MSK))
// Register R1 [0x1] -- REG1

enum reg1_fields_t {
    CP_GAIN_1_OFF = 0x15,
    CP_GAIN_1_MSK = 0x200000,
    N_OFF = 0x8,
    N_MSK = 0x1fff00,
};
#define GET_ADF4002B_CP_GAIN_1(x) (((x) & CP_GAIN_1_MSK) >> CP_GAIN_1_OFF)
#define GET_ADF4002B_N(x) (((x) & N_MSK) >> N_OFF)
#define SET_ADF4002B_CP_GAIN_1(p, f) (p) = ((p) & ~CP_GAIN_1_MSK) | (((f) << CP_GAIN_1_OFF) & CP_GAIN_1_MSK)
#define SET_ADF4002B_N(p, f) (p) = ((p) & ~N_MSK) | (((f) << N_OFF) & N_MSK)

#define MAKE_ADF4002B_REG1(cp_gain_1, n) MAKE_ADF4002B_REG_WR(REG1, \
    (((cp_gain_1) << CP_GAIN_1_OFF) & CP_GAIN_1_MSK) |  \
    (((n) << N_OFF) & N_MSK))
// Register R2 [0x2] -- REG2
enum fastlock_options {
    FASTLOCK_DISABLED = 0,
    FASTLOCK_DISABLED = 1,
    FASTLOCK_FS_MODE_1 = 2,
    FASTLOCK_FS_MODE_2 = 3,
};
enum muxout_options {
    MUXOUT_TRI_STATE = 0,
    MUXOUT_DIG_LD = 1,
    MUXOUT_N_DIV_OUT = 2,
    MUXOUT_DVDD = 3,
    MUXOUT_R_DIV_OUT = 4,
    MUXOUT_OPEND_LD = 5,
    MUXOUT_SDO = 6,
    MUXOUT_DGND = 7,
};
enum pdn_options {
    PDN_NORMAL = 0,
    PDN_ASYNC_PD = 1,
    PDN_NORMAL = 2,
    PDN_SYNC_PD = 3,
};

enum reg2_fields_t {
    CUR_CP_2_OFF = 0x12,
    CUR_CP_2_MSK = 0x1c0000,
    CUR_CP_1_OFF = 0xf,
    CUR_CP_1_MSK = 0x38000,
    PFD_TIMEOUT_OFF = 0xb,
    PFD_TIMEOUT_MSK = 0x7800,
    FASTLOCK_OFF = 0x9,
    FASTLOCK_MSK = 0x600,
    CP_TRI_STATE_OFF = 0x8,
    CP_TRI_STATE_MSK = 0x100,
    PD_POLARITY_OFF = 0x7,
    PD_POLARITY_MSK = 0x80,
    MUXOUT_OFF = 0x4,
    MUXOUT_MSK = 0x70,
    PDN_OFF = 0x3,
    PDN_MSK = 0x200008,
    CNTR_RST_OFF = 0x2,
    CNTR_RST_MSK = 0x4,
};
#define GET_ADF4002B_CUR_CP_2(x) (((x) & CUR_CP_2_MSK) >> CUR_CP_2_OFF)
#define GET_ADF4002B_CUR_CP_1(x) (((x) & CUR_CP_1_MSK) >> CUR_CP_1_OFF)
#define GET_ADF4002B_PFD_TIMEOUT(x) (((x) & PFD_TIMEOUT_MSK) >> PFD_TIMEOUT_OFF)
#define GET_ADF4002B_FASTLOCK(x) (((x) & FASTLOCK_MSK) >> FASTLOCK_OFF)
#define GET_ADF4002B_CP_TRI_STATE(x) (((x) & CP_TRI_STATE_MSK) >> CP_TRI_STATE_OFF)
#define GET_ADF4002B_PD_POLARITY(x) (((x) & PD_POLARITY_MSK) >> PD_POLARITY_OFF)
#define GET_ADF4002B_MUXOUT(x) (((x) & MUXOUT_MSK) >> MUXOUT_OFF)
#define GET_ADF4002B_PDN(x) ((((((x) & PDN_MSK) >> 3) & 0x1) << 0) | (((((x) & PDN_MSK) >> 21) & 0x1) << 1))
#define GET_ADF4002B_CNTR_RST(x) (((x) & CNTR_RST_MSK) >> CNTR_RST_OFF)
#define SET_ADF4002B_CUR_CP_2(p, f) (p) = ((p) & ~CUR_CP_2_MSK) | (((f) << CUR_CP_2_OFF) & CUR_CP_2_MSK)
#define SET_ADF4002B_CUR_CP_1(p, f) (p) = ((p) & ~CUR_CP_1_MSK) | (((f) << CUR_CP_1_OFF) & CUR_CP_1_MSK)
#define SET_ADF4002B_PFD_TIMEOUT(p, f) (p) = ((p) & ~PFD_TIMEOUT_MSK) | (((f) << PFD_TIMEOUT_OFF) & PFD_TIMEOUT_MSK)
#define SET_ADF4002B_FASTLOCK(p, f) (p) = ((p) & ~FASTLOCK_MSK) | (((f) << FASTLOCK_OFF) & FASTLOCK_MSK)
#define SET_ADF4002B_CP_TRI_STATE(p, f) (p) = ((p) & ~CP_TRI_STATE_MSK) | (((f) << CP_TRI_STATE_OFF) & CP_TRI_STATE_MSK)
#define SET_ADF4002B_PD_POLARITY(p, f) (p) = ((p) & ~PD_POLARITY_MSK) | (((f) << PD_POLARITY_OFF) & PD_POLARITY_MSK)
#define SET_ADF4002B_MUXOUT(p, f) (p) = ((p) & ~MUXOUT_MSK) | (((f) << MUXOUT_OFF) & MUXOUT_MSK)
#define SET_ADF4002B_PDN(p, f) (p) = ((p) & ~PDN_MSK) | ((((((f) >> 1) & 0x1) << 21) | ((((f) >> 0) & 0x1) << 3)) & PDN_MSK)
#define SET_ADF4002B_CNTR_RST(p, f) (p) = ((p) & ~CNTR_RST_MSK) | (((f) << CNTR_RST_OFF) & CNTR_RST_MSK)

#define MAKE_ADF4002B_REG2(cur_cp_2, cur_cp_1, pfd_timeout, fastlock, cp_tri_state, pd_polarity, muxout, pdn, cntr_rst) MAKE_ADF4002B_REG_WR(REG2, \
    (((cur_cp_2) << CUR_CP_2_OFF) & CUR_CP_2_MSK) |  \
    (((cur_cp_1) << CUR_CP_1_OFF) & CUR_CP_1_MSK) |  \
    (((pfd_timeout) << PFD_TIMEOUT_OFF) & PFD_TIMEOUT_MSK) |  \
    (((fastlock) << FASTLOCK_OFF) & FASTLOCK_MSK) |  \
    (((cp_tri_state) << CP_TRI_STATE_OFF) & CP_TRI_STATE_MSK) |  \
    (((pd_polarity) << PD_POLARITY_OFF) & PD_POLARITY_MSK) |  \
    (((muxout) << MUXOUT_OFF) & MUXOUT_MSK) |  \
    ((((((pdn) >> 1) & 0x1) << 21) | ((((pdn) >> 0) & 0x1) << 3)) & PDN_MSK) |  \
    (((cntr_rst) << CNTR_RST_OFF) & CNTR_RST_MSK))
// Register R3 [0x3] -- REG3
enum fastlock_options {
    FASTLOCK_DISABLED = 0,
    FASTLOCK_DISABLED = 1,
    FASTLOCK_FS_MODE_1 = 2,
    FASTLOCK_FS_MODE_2 = 3,
};
enum muxout_options {
    MUXOUT_TRI_STATE = 0,
    MUXOUT_DIG_LD = 1,
    MUXOUT_N_DIV_OUT = 2,
    MUXOUT_DVDD = 3,
    MUXOUT_R_DIV_OUT = 4,
    MUXOUT_OPEND_LD = 5,
    MUXOUT_SDO = 6,
    MUXOUT_DGND = 7,
};
enum pdn_options {
    PDN_NORMAL = 0,
    PDN_ASYNC_PD = 1,
    PDN_NORMAL = 2,
    PDN_SYNC_PD = 3,
};

enum reg3_fields_t {
    CUR_CP_2_OFF = 0x12,
    CUR_CP_2_MSK = 0x1c0000,
    CUR_CP_1_OFF = 0xf,
    CUR_CP_1_MSK = 0x38000,
    PFD_TIMEOUT_OFF = 0xb,
    PFD_TIMEOUT_MSK = 0x7800,
    FASTLOCK_OFF = 0x9,
    FASTLOCK_MSK = 0x600,
    CP_TRI_STATE_OFF = 0x8,
    CP_TRI_STATE_MSK = 0x100,
    PD_POLARITY_OFF = 0x7,
    PD_POLARITY_MSK = 0x80,
    MUXOUT_OFF = 0x4,
    MUXOUT_MSK = 0x70,
    PDN_OFF = 0x3,
    PDN_MSK = 0x200008,
    CNTR_RST_OFF = 0x2,
    CNTR_RST_MSK = 0x4,
};
#define GET_ADF4002B_CUR_CP_2(x) (((x) & CUR_CP_2_MSK) >> CUR_CP_2_OFF)
#define GET_ADF4002B_CUR_CP_1(x) (((x) & CUR_CP_1_MSK) >> CUR_CP_1_OFF)
#define GET_ADF4002B_PFD_TIMEOUT(x) (((x) & PFD_TIMEOUT_MSK) >> PFD_TIMEOUT_OFF)
#define GET_ADF4002B_FASTLOCK(x) (((x) & FASTLOCK_MSK) >> FASTLOCK_OFF)
#define GET_ADF4002B_CP_TRI_STATE(x) (((x) & CP_TRI_STATE_MSK) >> CP_TRI_STATE_OFF)
#define GET_ADF4002B_PD_POLARITY(x) (((x) & PD_POLARITY_MSK) >> PD_POLARITY_OFF)
#define GET_ADF4002B_MUXOUT(x) (((x) & MUXOUT_MSK) >> MUXOUT_OFF)
#define GET_ADF4002B_PDN(x) ((((((x) & PDN_MSK) >> 3) & 0x1) << 0) | (((((x) & PDN_MSK) >> 21) & 0x1) << 1))
#define GET_ADF4002B_CNTR_RST(x) (((x) & CNTR_RST_MSK) >> CNTR_RST_OFF)
#define SET_ADF4002B_CUR_CP_2(p, f) (p) = ((p) & ~CUR_CP_2_MSK) | (((f) << CUR_CP_2_OFF) & CUR_CP_2_MSK)
#define SET_ADF4002B_CUR_CP_1(p, f) (p) = ((p) & ~CUR_CP_1_MSK) | (((f) << CUR_CP_1_OFF) & CUR_CP_1_MSK)
#define SET_ADF4002B_PFD_TIMEOUT(p, f) (p) = ((p) & ~PFD_TIMEOUT_MSK) | (((f) << PFD_TIMEOUT_OFF) & PFD_TIMEOUT_MSK)
#define SET_ADF4002B_FASTLOCK(p, f) (p) = ((p) & ~FASTLOCK_MSK) | (((f) << FASTLOCK_OFF) & FASTLOCK_MSK)
#define SET_ADF4002B_CP_TRI_STATE(p, f) (p) = ((p) & ~CP_TRI_STATE_MSK) | (((f) << CP_TRI_STATE_OFF) & CP_TRI_STATE_MSK)
#define SET_ADF4002B_PD_POLARITY(p, f) (p) = ((p) & ~PD_POLARITY_MSK) | (((f) << PD_POLARITY_OFF) & PD_POLARITY_MSK)
#define SET_ADF4002B_MUXOUT(p, f) (p) = ((p) & ~MUXOUT_MSK) | (((f) << MUXOUT_OFF) & MUXOUT_MSK)
#define SET_ADF4002B_PDN(p, f) (p) = ((p) & ~PDN_MSK) | ((((((f) >> 1) & 0x1) << 21) | ((((f) >> 0) & 0x1) << 3)) & PDN_MSK)
#define SET_ADF4002B_CNTR_RST(p, f) (p) = ((p) & ~CNTR_RST_MSK) | (((f) << CNTR_RST_OFF) & CNTR_RST_MSK)

#define MAKE_ADF4002B_REG3(cur_cp_2, cur_cp_1, pfd_timeout, fastlock, cp_tri_state, pd_polarity, muxout, pdn, cntr_rst) MAKE_ADF4002B_REG_WR(REG3, \
    (((cur_cp_2) << CUR_CP_2_OFF) & CUR_CP_2_MSK) |  \
    (((cur_cp_1) << CUR_CP_1_OFF) & CUR_CP_1_MSK) |  \
    (((pfd_timeout) << PFD_TIMEOUT_OFF) & PFD_TIMEOUT_MSK) |  \
    (((fastlock) << FASTLOCK_OFF) & FASTLOCK_MSK) |  \
    (((cp_tri_state) << CP_TRI_STATE_OFF) & CP_TRI_STATE_MSK) |  \
    (((pd_polarity) << PD_POLARITY_OFF) & PD_POLARITY_MSK) |  \
    (((muxout) << MUXOUT_OFF) & MUXOUT_MSK) |  \
    ((((((pdn) >> 1) & 0x1) << 21) | ((((pdn) >> 0) & 0x1) << 3)) & PDN_MSK) |  \
    (((cntr_rst) << CNTR_RST_OFF) & CNTR_RST_MSK))
