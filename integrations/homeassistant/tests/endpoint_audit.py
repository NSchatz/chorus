"""The checker behind `test_no_unauthenticated_endpoint.py`.

Two halves. The static half reads Python source with `ast` and reports every
construct that could expose an endpoint without authentication. The runtime
half compares Home Assistant's aiohttp router and webhook registry before and
after the integration is set up and reports every route or webhook that was
added and is not provably authenticated.

The rule (brief section 4.8): every HTTP view requires auth, every webhook is
local-only, and the integration serves no static path and injects no script.
"""

from __future__ import annotations

import ast
from collections.abc import Iterable
from dataclasses import dataclass
from functools import partial
from pathlib import Path
from typing import Any

from aiohttp import web
from aiohttp.web_urldispatcher import AbstractResource, StaticResource
from homeassistant.components.webhook import DOMAIN as WEBHOOK_DOMAIN
from homeassistant.core import HomeAssistant
from homeassistant.helpers.http import HomeAssistantView

# Static paths are served without authentication. A path is allowed only by
# being listed here, as (file name, URL path). Empty: the integration has none.
STATIC_PATH_ALLOWLIST: frozenset[tuple[str, str]] = frozenset()

_STATIC_CALLS = {"async_register_static_paths", "register_static_path"}
# What puts a route on an aiohttp router (or a redirect on Home Assistant's)
# without a view the scan can read: the scan cannot tell that such a route
# requires auth, so each one is a finding. A view goes through `register_view`.
_ROUTER_CALLS = {
    "add_delete",
    "add_get",
    "add_head",
    "add_patch",
    "add_post",
    "add_put",
    "add_resource",
    "add_route",
    "add_routes",
    "add_static",
    "add_subapp",
    "add_view",
    "register_redirect",
}


@dataclass(frozen=True)
class Finding:
    """One thing that breaks the rule: what kind, its name, and where."""

    kind: str
    name: str
    where: str


def _name(node: ast.AST) -> str:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        return node.attr
    if isinstance(node, ast.Call):
        return _name(node.func)
    return ""


def _is_false(node: ast.AST) -> bool:
    return isinstance(node, ast.Constant) and node.value is False


def _is_true(node: ast.AST) -> bool:
    return isinstance(node, ast.Constant) and node.value is True


def _static_url(call: ast.Call) -> str:
    for arg in [*call.args, *(k.value for k in call.keywords)]:
        if isinstance(arg, ast.Constant) and isinstance(arg.value, str):
            return arg.value
    return "?"


def scan_source(
    source: str,
    filename: str,
    static_allowlist: Iterable[tuple[str, str]] = STATIC_PATH_ALLOWLIST,
) -> list[Finding]:
    """Report every unauthenticated-endpoint construct in one Python source."""
    tree = ast.parse(source, filename)
    allowed = set(static_allowlist)
    findings: list[Finding] = []
    view_classes: dict[str, bool] = {}  # class name -> requires auth
    webhook_names = {"webhook"}
    webhook_functions: set[str] = set()

    for node in ast.walk(tree):
        if isinstance(node, ast.ImportFrom) and node.module is not None:
            for alias in node.names:
                bound = alias.asname or alias.name
                if alias.name == "webhook":
                    webhook_names.add(bound)
                if node.module.endswith("webhook") and alias.name == "async_register":
                    webhook_functions.add(bound)
        if isinstance(node, ast.Import):
            for alias in node.names:
                if alias.name.endswith(".webhook") and alias.asname:
                    webhook_names.add(alias.asname)

    for node in ast.walk(tree):
        where = f"{filename}:{getattr(node, 'lineno', 0)}"
        if isinstance(node, ast.ClassDef):
            is_view = any(_name(base).endswith("View") for base in node.bases)
            requires_auth = True
            for statement in ast.walk(node):
                targets: list[ast.expr] = []
                value: ast.AST | None = None
                if isinstance(statement, ast.Assign):
                    targets, value = statement.targets, statement.value
                elif isinstance(statement, ast.AnnAssign) and statement.value:
                    targets, value = [statement.target], statement.value
                if value is None:
                    continue
                if any(_name(t) == "requires_auth" for t in targets) and not _is_true(
                    value
                ):
                    requires_auth = False
            if is_view or not requires_auth:
                view_classes[node.name] = requires_auth
            if not requires_auth:
                findings.append(Finding("view-without-auth", node.name, where))
        elif isinstance(node, (ast.Assign, ast.AnnAssign)):
            # `requires_auth` cleared outside a class body (on an instance, say).
            targets = node.targets if isinstance(node, ast.Assign) else [node.target]
            value = node.value
            if (
                value is not None
                and not _is_true(value)
                and any(
                    isinstance(t, ast.Attribute) and t.attr == "requires_auth"
                    for t in targets
                )
            ):
                findings.append(
                    Finding("view-without-auth", ast.unparse(targets[0]), where)
                )
        elif isinstance(node, ast.Call):
            called = _name(node.func)
            if called in _STATIC_CALLS or called == "StaticPathConfig":
                if called in _STATIC_CALLS and any(
                    isinstance(inner, ast.Call) and _name(inner) == "StaticPathConfig"
                    for arg in node.args
                    for inner in ast.walk(arg)
                ):
                    # The configuration inside it is reported, by its path.
                    continue
                url = _static_url(node)
                if (Path(filename).name, url) not in allowed:
                    findings.append(Finding("static-path", url, where))
            elif called == "add_extra_js_url":
                findings.append(Finding("extra-js-url", _static_url(node), where))
            elif called in _ROUTER_CALLS and isinstance(node.func, ast.Attribute):
                findings.append(
                    Finding("route-not-auditable", ast.unparse(node.func), where)
                )
            elif called == "register_view":
                viewed = _name(node.args[0]) if node.args else ""
                if viewed not in view_classes:
                    findings.append(Finding("view-not-auditable", viewed or "?", where))
            elif (
                called == "async_register"
                and isinstance(node.func, ast.Attribute)
                and _name(node.func.value) in webhook_names
            ) or (isinstance(node.func, ast.Name) and called in webhook_functions):
                local_only = next(
                    (k.value for k in node.keywords if k.arg == "local_only"), None
                )
                if local_only is None or not _is_true(local_only):
                    webhook = (
                        node.args[3].value
                        if len(node.args) > 3 and isinstance(node.args[3], ast.Constant)
                        else "?"
                    )
                    findings.append(Finding("webhook-not-local-only", webhook, where))
    # A view registered before its class appears later in the file is fine;
    # one that is never defined here was reported above.
    return findings


def scan_tree(root: Path) -> tuple[list[Finding], int]:
    """Scan every Python file under a directory; return findings and file count."""
    findings: list[Finding] = []
    files = sorted(root.rglob("*.py"))
    for path in files:
        findings += scan_source(path.read_text(encoding="utf-8"), str(path))
    return findings, len(files)


# --- the runtime half ----------------------------------------------------------


@dataclass(frozen=True)
class Snapshot:
    """The router's resources and the webhook registry at one moment."""

    resources: frozenset[int]
    webhooks: frozenset[str]


def _router(hass: HomeAssistant) -> web.UrlDispatcher:
    return hass.http.app.router


def _webhooks(hass: HomeAssistant) -> dict[str, Any]:
    return hass.data.get(WEBHOOK_DOMAIN, {})  # the registry's HassKey is its domain


def snapshot(hass: HomeAssistant) -> Snapshot:
    """Record what is registered now."""
    return Snapshot(
        frozenset(id(resource) for resource in _router(hass).resources()),
        frozenset(_webhooks(hass)),
    )


def _view_of(handler: Any) -> HomeAssistantView | None:
    """Return the view a route's handler was made from, if it is a view's."""
    for cell in getattr(handler, "__closure__", None) or ():
        try:
            contents = cell.cell_contents
        except ValueError:
            continue
        if isinstance(contents, HomeAssistantView):
            return contents
    return None


def _module(handler: Any) -> str:
    owner = getattr(handler, "__self__", None)
    if owner is not None:
        return str(type(owner).__module__)
    return str(getattr(handler, "__module__", ""))


def _describe(resource: AbstractResource) -> str:
    return getattr(resource, "canonical", None) or repr(resource)


def audit_added(
    hass: HomeAssistant, before: Snapshot, webhook_domain: str
) -> tuple[list[Finding], int, int]:
    """Report what was added since `before` and is not provably authenticated.

    Returns the findings, the number of resources (URL paths) added to the
    router and the number of webhooks added. Every webhook added since `before`
    is audited, whatever domain it was registered under: the caller sets up
    what the integration depends on before it takes `before`, so what is added
    afterwards is the integration's, and one registered under another domain's
    name is reported for that as well.
    """
    found: set[Finding] = set()
    routes = 0
    for resource in _router(hass).resources():
        if id(resource) in before.resources:
            continue
        routes += 1
        if isinstance(resource, StaticResource):
            found.add(Finding("static-path", _describe(resource), "runtime"))
            continue
        for route in resource:
            handler = route.handler
            if route.method == "OPTIONS" and _module(handler).startswith(
                "aiohttp_cors"
            ):
                # The CORS preflight answer Home Assistant adds beside a route.
                continue
            if isinstance(handler, partial) and handler.func.__name__.startswith(
                "_serve_file"
            ):
                found.add(Finding("static-path", _describe(resource), "runtime"))
                continue
            view = _view_of(handler)
            if view is None:
                found.add(Finding("view-not-auditable", _describe(resource), "runtime"))
            elif view.requires_auth is not True:
                found.add(
                    Finding(
                        "view-without-auth",
                        f"{type(view).__name__} {_describe(resource)}",
                        "runtime",
                    )
                )
    findings = sorted(found, key=lambda f: (f.kind, f.name))
    webhooks = 0
    for webhook_id, data in _webhooks(hass).items():
        if webhook_id in before.webhooks:
            continue
        webhooks += 1
        if data.domain != webhook_domain:
            findings.append(Finding("webhook-foreign-domain", webhook_id, "runtime"))
        if data.local_only is not True:
            findings.append(Finding("webhook-not-local-only", webhook_id, "runtime"))
    return findings, routes, webhooks
