/* The one translation unit that compiles dr_flac's implementation. chorus's own
 * file (MIT OR Apache-2.0), not upstream's: dr_flac is a single header whose
 * implementation is compiled where DR_FLAC_IMPLEMENTATION is defined. Its
 * configuration (DR_FLAC_NO_STDIO, DR_FLAC_NO_OGG, DR_FLAC_NO_WCHAR,
 * DR_FLAC_NO_SIMD) comes from the build, so every unit that includes the header
 * sees the same one; third_party/README.md says why each is set. */
#define DR_FLAC_IMPLEMENTATION
#include "dr_flac.h"
