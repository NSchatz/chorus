"""Announcements: a clip Home Assistant serves, played by the chorus server.

The media player's `play_media` with `announce` and the voice satellite's
replies and announcements all end here, so the rule is in one place: the
server fetches only from Home Assistant itself (and holds its own list of
origins as well), and a refusal says which of the two lists was the reason.
"""

from __future__ import annotations

from homeassistant.components.media_player import async_process_play_media_url
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import HomeAssistantError, ServiceValidationError
from homeassistant.helpers.network import NoURLAvailableError, get_url
from yarl import URL

from ._aiochorus import State, commands
from .const import DOMAIN
from .coordinator import ChorusCoordinator

type Origin = tuple[str, str | None, int | None]


def origin_of(url: str) -> Origin | None:
    """Return the scheme, host and port of an http(s) URL, or None."""
    try:
        parsed = URL(url)
    except ValueError:
        return None
    if parsed.scheme not in ("http", "https") or not parsed.host:
        return None
    return (parsed.scheme, parsed.host, parsed.port)


def own_origins(hass: HomeAssistant) -> set[Origin]:
    """Return Home Assistant's own internal and external origins."""
    origins = set()
    for internal in (True, False):
        try:
            url = get_url(
                hass,
                allow_internal=internal,
                allow_external=not internal,
                allow_cloud=False,
            )
        except NoURLAvailableError:
            continue
        if (origin := origin_of(url)) is not None:
            origins.add(origin)
    return origins


def server_origins(coordinator: ChorusCoordinator) -> set[Origin | None]:
    """Return the origins the server said it announces from."""
    return {origin_of(listed) for listed in coordinator.server.announce_origins}


async def async_announce(
    hass: HomeAssistant,
    coordinator: ChorusCoordinator,
    target: str,
    media_id: str,
    thousandths: int | None = None,
) -> State:
    """Have the server play a clip Home Assistant serves in a room or a group.

    `media_id` is a URL or a path of this Home Assistant. Returns the server's
    answer as soon as the announcement has started; waiting for its end is
    `ChorusCoordinator.async_wait_for_announcement`.
    """
    # Refused here, before any request leaves: the server fetches only from
    # Home Assistant itself (and holds its own list as well).
    try:
        url = async_process_play_media_url(hass, media_id)
    except ValueError:
        url = ""
    origin = origin_of(url)
    if origin is None or origin not in own_origins(hass):
        raise ServiceValidationError(
            translation_domain=DOMAIN, translation_key="announce_origin"
        )
    try:
        return await coordinator.async_command(
            commands.announce(target, url, thousandths)
        )
    except HomeAssistantError as err:
        if err.translation_key != "refused_url":
            raise
        # The server refused the address. If its own list of announce
        # origins does not hold this Home Assistant, say that: it is a
        # server setting (--announce-origin), not something about the clip.
        await coordinator.async_refresh_server()
        if origin in server_origins(coordinator):
            raise
        raise HomeAssistantError(
            translation_domain=DOMAIN,
            translation_key="announce_origin_not_on_server",
            translation_placeholders={
                "origin": f"{origin[0]}://{origin[1]}:{origin[2]}"
            },
        ) from err
