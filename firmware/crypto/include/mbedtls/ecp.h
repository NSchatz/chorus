/* The pinned ESP-IDF v6.1 tree's TF-PSA-Crypto includes "mbedtls/ecp.h",
 * which ESP-IDF itself supplies from its mbedtls component's port/include
 * (a wrapper adding ESP32-only functions). The host build has no ESP32
 * functions, so its wrapper is the library's own header and nothing more. */

#ifndef CHORUS_HOST_MBEDTLS_ECP_H
#define CHORUS_HOST_MBEDTLS_ECP_H

#include "mbedtls/private/ecp.h"

#endif
