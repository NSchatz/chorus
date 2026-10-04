"""Constants of the chorus integration."""

from __future__ import annotations

from typing import Final

DOMAIN: Final = "chorus"

# docs/decisions/0027: the control port.
DEFAULT_PORT: Final = 4020

# The catalog version this integration speaks (docs/control-plane.md).
CATALOG_VERSION: Final = 2

# Media ids this integration plays and offers when browsing.
MEDIA_ID_ROOT: Final = "chorus://input"
MEDIA_ID_PREFIX: Final = "chorus://input/"
MEDIA_TYPE_INPUT: Final = "chorus_input"

# One press of volume up or down, in thousandths of full scale.
VOLUME_STEP_THOUSANDTHS: Final = 50
