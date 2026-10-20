# 0253: the config flow imports probatio, Home Assistant's validation engine since 2026.10

- Status: decided, 2026-10-19 (harness task 1451, the harness pin's move to Home Assistant
  2026.10.2)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `integrations/homeassistant/custom_components/chorus/config_flow.py`

## Context

The homelab moved its Home Assistant pin to 2026.10.2, so the harness pin moved with it
(`docs/home-assistant.md`, "The test harness and its pin"). Core 2026.10 replaced voluptuous
with probatio: `pyproject.toml` at tag 2026.10.2 requires `probatio==0.13.0` and no longer
requires voluptuous, `data_entry_flow.py` types a form's `data_schema` as `probatio.Schema`,
and `homeassistant/__init__.py` calls `probatio.compat.install_as_voluptuous()`, so on a
running Home Assistant an `import voluptuous` is probatio under another name. The config flow
imported `voluptuous as vol`; in the harness the real voluptuous 0.16.0 still arrived
transitively (through annotatedyaml and hass-nabucasa), so mypy checked the flow against a
package Home Assistant does not run.

## Decision

The config flow imports `probatio` by its own name, as core's components do (core's
`pyproject.toml` bans the alias `vol` for it under
`[tool.ruff.lint.flake8-import-conventions.banned-aliases]`). The port is `probatio.Port()`,
which core's ruff configuration names in place of `cv.port` (the same validator:
`cv.port = probatio.Port()`); `cv.string`, which core does not ban, stays. probatio is a direct
requirement of the `homeassistant` package the integration runs inside, as core's helpers are,
so the manifest's `requirements` stays empty.

## Not chosen

- **Declaring voluptuous** in the manifest or the `dev` group. On Home Assistant 2026.10 the
  name is aliased to probatio at import, so a declared voluptuous would be installed and never
  run, and the tests would type-check against it.
- **Keeping `import voluptuous`** and relying on the alias. It works today, but core's ruff
  configuration bans the name ("use probatio instead") and the alias is a bridge for
  integrations not yet migrated.

## What was read

- Home Assistant core (Apache-2.0) at tag 2026.10.2, commit
  8d7fd8b68677c2585be36ec86043308f403875b8, read 2026-10-19: `pyproject.toml`,
  `homeassistant/__init__.py`, `homeassistant/data_entry_flow.py`,
  `homeassistant/helpers/config_validation.py`, `script/hassfest/mypy_config.py`,
  `script/hassfest/quality_scale.py`.
- https://pypi.org/pypi/pytest-homeassistant-custom-component/0.13.373/json, read 2026-10-19.
