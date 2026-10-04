"""A speaker's firmware: what it runs, the verified staged image, and the install.

Nothing here installs anything on its own. The one place this integration
writes `firmware_install` is `async_install`, which Home Assistant calls for
its `update.install` action and for nothing else: setting the entry up, a
state message, a reconnect and a reload read the state and send nothing.
"""

from __future__ import annotations

from typing import Any

from homeassistant.components.update import (
    UpdateDeviceClass,
    UpdateEntity,
    UpdateEntityFeature,
)
from homeassistant.core import HomeAssistant, callback
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from ._aiochorus import Speaker, commands
from .const import DOMAIN
from .coordinator import ChorusConfigEntry, ChorusCoordinator, speaker_identifier
from .entity import ChorusSpeakerEntity

# Commands go to the server one at a time per platform.
PARALLEL_UPDATES = 1

# The refusals of `firmware_install` a person can do something about, by the
# name the catalog starts their detail with. Any other is mapped by its field.
REFUSALS = {
    "owner-not-at-bench": "firmware_owner_not_at_bench",
    "busy": "firmware_busy",
    "image-not-verified": "firmware_image_not_verified",
}


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ChorusConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    """Add an update entity per speaker that reported its firmware, as they do."""
    coordinator = entry.runtime_data
    known: set[str] = set()

    @callback
    def _add_new() -> None:
        reported = [s for s in coordinator.data.speakers if s.firmware is not None]
        new = [
            ChorusFirmwareUpdate(coordinator, speaker)
            for speaker in reported
            if speaker.id not in known
        ]
        known.clear()
        known.update(speaker.id for speaker in reported)
        if new:
            async_add_entities(new)

    _add_new()
    entry.async_on_unload(coordinator.async_add_listener(_add_new))


class ChorusFirmwareUpdate(ChorusSpeakerEntity, UpdateEntity):
    """The firmware of one adopted speaker."""

    _attr_translation_key = "firmware"
    _attr_device_class = UpdateDeviceClass.FIRMWARE
    _attr_supported_features = (
        UpdateEntityFeature.INSTALL | UpdateEntityFeature.PROGRESS
    )
    _attr_title = "chorus speaker firmware"

    def __init__(self, coordinator: ChorusCoordinator, speaker: Speaker) -> None:
        """Name the entity for the speaker."""
        super().__init__(coordinator, speaker)
        self._attr_unique_id = (
            f"{speaker_identifier(coordinator.server.id, speaker.id)}:firmware"
        )

    @property
    def available(self) -> bool:
        """Available while the server has the speaker and knows what it runs."""
        return super().available and self.firmware is not None

    @property
    def installed_version(self) -> str | None:
        """Return the version the speaker runs."""
        running = self.firmware
        return None if running is None else running.version

    @property
    def latest_version(self) -> str | None:
        """Return the verified staged image's version, or the running one.

        Only an image the server verified, for this speaker's board and above
        the version it runs, is ever named here; with none, the latest version
        is the installed one.
        """
        offer = self.coordinator.data.firmware_offer(self._speaker_id)
        return self.installed_version if offer is None else offer.version

    def version_is_newer(self, latest_version: str, installed_version: str) -> bool:
        """Say an update is available whenever a staged image is offered.

        `latest_version` differs from the installed one only when it is the
        offered image's, and which image is above the running version was
        decided when it was picked: no second comparison here can disagree.
        """
        return latest_version != installed_version

    @property
    def in_progress(self) -> bool:
        """Return whether the speaker has an install in progress."""
        running = self.firmware
        return running is not None and running.busy

    @property
    def update_percentage(self) -> int | None:
        """Return how much of the image the speaker has received.

        None once the transfer is over (the speaker is verifying, rebooting or
        running the image on trial): there is nothing left to count.
        """
        running = self.firmware
        if running is None or running.state not in ("requested", "receiving"):
            return None
        if running.size <= 0:
            return 0
        return min(100, running.received * 100 // running.size)

    @property
    def extra_state_attributes(self) -> dict[str, Any] | None:
        """Return what the speaker is doing or how its last install ended."""
        running = self.firmware
        if running is None:
            return None
        return {
            "install_state": running.state,
            "reason": running.reason,
            "image": running.image,
            "image_version": running.image_version,
            "board": running.board,
        }

    async def async_install(
        self, version: str | None, backup: bool, **kwargs: Any
    ) -> None:
        """Ask the server to install the verified staged image on this speaker."""
        offer = self.coordinator.data.firmware_offer(self._speaker_id)
        if offer is None:
            raise HomeAssistantError(
                translation_domain=DOMAIN,
                translation_key="firmware_nothing_to_install",
                translation_placeholders={"speaker": self._speaker_id},
            )
        await self.coordinator.async_command(
            commands.firmware_install(self._speaker_id, offer.name), REFUSALS
        )
