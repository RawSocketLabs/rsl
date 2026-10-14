// Exports libusdr's header-only rtsa_calc_depth so the oracle can call the real C body.
// The CPU capability query is pinned to the generic (scalar) build, the one usdr ports;
// cpu_vcap_align itself is libusdr's.
#include "vbase.h"
#define cpu_vcap_get() OPT_GENERIC
#include "rtsa_functions.h"

void rsl_oracle_rtsa_calc_depth(fft_rtsa_settings_t* st)
{
    rtsa_calc_depth(st);
}
