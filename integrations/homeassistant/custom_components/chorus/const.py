"""Constants of the chorus integration."""

from __future__ import annotations

from datetime import timedelta
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

# The diagnostic sensors read `GET /metrics` (docs/telemetry.md), which the
# server renders on every request. One scrape a minute while at least one
# diagnostic sensor is enabled, none otherwise; a refresh asked for sooner than
# the minimum gap after the last scrape is answered from that scrape.
METRICS_SCAN_INTERVAL: Final = timedelta(seconds=60)
METRICS_MIN_GAP_SECONDS: Final = 55.0

# A room's visualizer sensor (docs/decisions, "the Home Assistant visualizer
# entity"). The server sends a subscriber at most ten frames a second
# (docs/visualizer.md, "The rate cap"); the entity writes its state at most
# once in this many seconds whatever arrives, the latest frame superseding the
# ones before. ASSUMED, not measured: five writes a second, half the server's
# rate, which still shows each beat of music up to 300 beats a minute on its
# own write.
VISUALIZER_MIN_WRITE_INTERVAL: Final = 0.2
# With no frame for this long the entity is idle: the room was given nothing
# to play, so no silent frame came to say so. ASSUMED: twenty frames missed at
# the server's rate.
VISUALIZER_IDLE_AFTER: Final = 2.0
