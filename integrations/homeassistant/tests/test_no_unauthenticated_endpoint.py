"""No unauthenticated endpoint (brief section 4.8; the goal's line C).

Every HTTP view the integration registers requires auth and every webhook it
registers is local-only. It registers neither today, and these tests are what
would catch the first one that did it wrong:

1. at run time, the router and the webhook registry are compared before and
   after the integration and every one of its platforms is set up;
2. statically, every file of the integration is read for the constructs;
3. the same checker is fed a bad view, a webhook that is not local-only and a
   static path, and must name each one, so neither half passes by seeing nothing.
"""

from __future__ import annotations

from pathlib import Path

from aiohttp import web
from homeassistant.components import webhook
from homeassistant.components.http import StaticPathConfig
from homeassistant.config_entries import ConfigEntryState
from homeassistant.core import HomeAssistant
from homeassistant.helpers.http import HomeAssistantView
from homeassistant.setup import async_setup_component
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.chorus import PLATFORMS
from custom_components.chorus.const import DOMAIN

from .conftest import wait_for
from .endpoint_audit import Finding, audit_added, scan_source, scan_tree, snapshot
from .fake_server import FakeChorusServer

INTEGRATION = Path(__file__).resolve().parents[1] / "custom_components" / "chorus"


async def test_runtime_every_route_requires_auth_and_every_webhook_is_local_only(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    """Set everything up with http and webhook loaded and audit what was added."""
    assert await async_setup_component(hass, "http", {})
    assert await async_setup_component(hass, "webhook", {})
    # What the integration depends on registers its own views (media_source's,
    # media_player's): those are Home Assistant's, so they are set up first and
    # the difference below is the integration's alone.
    for dependency in ("media_source", *PLATFORMS):
        assert await async_setup_component(hass, str(dependency), {})
    await hass.async_block_till_done()
    before = snapshot(hass)

    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    await wait_for(lambda: server.subscribers == 1)
    assert entry.state is ConfigEntryState.LOADED
    # Every platform of the integration is loaded, so nothing is left to register.
    loaded = {
        platform.domain
        for platform in hass.data["entity_platform"][DOMAIN]
        if platform.config_entry is entry
    }
    assert loaded == {str(p) for p in PLATFORMS}

    findings, routes, webhooks = audit_added(hass, before, DOMAIN)
    assert findings == []
    # Today the integration registers no view and no webhook at all.
    assert (routes, webhooks) == (0, 0)


def test_static_scan_of_every_file_of_the_integration() -> None:
    findings, files = scan_tree(INTEGRATION)
    assert files >= 14, "the scan did not find the integration's files"
    assert findings == []


BAD_VIEW = """
from homeassistant.helpers.http import HomeAssistantView

class OpenArtView(HomeAssistantView):
    url = "/api/chorus/art"
    name = "api:chorus:art"
    requires_auth = False

    async def get(self, request):
        return None

class ClosedView(HomeAssistantView):
    url = "/api/chorus/closed"
    name = "api:chorus:closed"

def setup(hass, opened):
    hass.http.register_view(OpenArtView())
    hass.http.register_view(ClosedView)
    hass.http.register_view(opened)
    opened.requires_auth = False
"""

BAD_WEBHOOK = """
from homeassistant.components import webhook
from homeassistant.components.webhook import async_register as register_hook

def setup(hass, handler):
    webhook.async_register(hass, "chorus", "Open", "chorus-open", handler)
    webhook.async_register(
        hass, "chorus", "Explicit", "chorus-explicit", handler, local_only=False
    )
    register_hook(hass, "chorus", "Bare", "chorus-bare", handler)
    webhook.async_register(
        hass, "chorus", "Good", "chorus-good", handler, local_only=True
    )
"""

BAD_STATIC = """
from homeassistant.components.frontend import add_extra_js_url
from homeassistant.components.http import StaticPathConfig

async def setup(hass):
    await hass.http.async_register_static_paths(
        [StaticPathConfig("/chorus_static", "/config/www", True)]
    )
    hass.http.register_static_path("/chorus_old", "/config/old")
    add_extra_js_url(hass, "/chorus_static/card.js")
"""


def test_the_static_checker_names_a_view_without_auth() -> None:
    findings = scan_source(BAD_VIEW, "bad_view.py")
    assert {(f.kind, f.name) for f in findings} == {
        ("view-without-auth", "OpenArtView"),
        ("view-not-auditable", "opened"),
        ("view-without-auth", "opened.requires_auth"),
    }


def test_the_static_checker_names_a_webhook_that_is_not_local_only() -> None:
    findings = scan_source(BAD_WEBHOOK, "bad_webhook.py")
    assert [(f.kind, f.name) for f in findings] == [
        ("webhook-not-local-only", "chorus-open"),
        ("webhook-not-local-only", "chorus-explicit"),
        ("webhook-not-local-only", "chorus-bare"),
    ]


def test_the_static_checker_names_a_static_path_and_an_injected_script() -> None:
    findings = scan_source(BAD_STATIC, "bad_static.py")
    assert {(f.kind, f.name) for f in findings} == {
        ("static-path", "/chorus_static"),
        ("static-path", "/chorus_old"),
        ("extra-js-url", "/chorus_static/card.js"),
    }
    # A static path passes only by being on the allowlist, by file and path.
    allowed = scan_source(
        BAD_STATIC,
        "bad_static.py",
        static_allowlist={
            ("bad_static.py", "/chorus_static"),
            ("bad_static.py", "/chorus_old"),
        },
    )
    assert [(f.kind, f.name) for f in allowed] == [
        ("extra-js-url", "/chorus_static/card.js")
    ]


async def test_the_runtime_checker_names_each_bad_registration(
    hass: HomeAssistant, tmp_path: Path, socket_enabled: None
) -> None:
    """Register one of each wrong thing and one of each right thing."""
    assert await async_setup_component(hass, "http", {})
    assert await async_setup_component(hass, "webhook", {})
    await hass.async_block_till_done()
    before = snapshot(hass)

    class OpenArtView(HomeAssistantView):
        url = "/api/chorus/art"
        name = "api:chorus:art"
        requires_auth = False

        async def get(self, request: web.Request) -> web.Response:
            return web.Response()

    class ClosedView(HomeAssistantView):
        url = "/api/chorus/closed"
        name = "api:chorus:closed"

        async def get(self, request: web.Request) -> web.Response:
            return web.Response()

    async def bare(request: web.Request) -> web.Response:
        return web.Response()

    async def hook(hass: HomeAssistant, webhook_id: str, request: web.Request) -> None:
        return None

    hass.http.register_view(OpenArtView())
    hass.http.register_view(ClosedView())
    hass.http.app.router.add_get("/api/chorus/bare", bare)
    await hass.http.async_register_static_paths(
        [StaticPathConfig("/chorus_static", str(tmp_path), False)]
    )
    webhook.async_register(hass, DOMAIN, "Open", "chorus-open", hook)
    webhook.async_register(hass, DOMAIN, "Good", "chorus-good", hook, local_only=True)
    webhook.async_register(hass, "other", "Other", "other-open", hook)

    findings, routes, webhooks = audit_added(hass, before, DOMAIN)
    assert set(findings) == {
        Finding("view-without-auth", "OpenArtView /api/chorus/art", "runtime"),
        Finding("view-not-auditable", "/api/chorus/bare", "runtime"),
        Finding("static-path", "/chorus_static", "runtime"),
        Finding("webhook-not-local-only", "chorus-open", "runtime"),
    }
    # Four URL paths: two views, a bare route, and the static path, which Home
    # Assistant registers as a directory resource and a route of the same path.
    assert routes == 5
    assert webhooks == 2
