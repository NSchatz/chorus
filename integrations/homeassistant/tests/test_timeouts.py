"""The per-test limit: a test past a minute fails, named, and the run goes on.

pytest-timeout holds it (`timeout = 60`, `timeout_method = "signal"` in
pyproject.toml); a longer test has its own `@pytest.mark.timeout`.
"""

from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys

import pytest


def test_ha_default_timeout_is_60(request: pytest.FixtureRequest) -> None:
    """The suite's own limit is a minute."""
    assert float(request.config.getini("timeout")) == 60


def test_ha_sleeping_test_fails_with_its_name(tmp_path: Path) -> None:
    """A test past its limit fails, and the output names it.

    The child runs under the same plugins, so it takes this suite's asyncio mode:
    the harness's autouse fixtures are async.
    """
    test_file = tmp_path / "test_sleep.py"
    test_file.write_text("import time\n\n\ndef test_sleeps():\n    time.sleep(5)\n")
    env = {k: v for k, v in os.environ.items() if k != "PYTEST_ADDOPTS"}
    result = subprocess.run(  # noqa: S603
        [
            sys.executable,
            "-m",
            "pytest",
            str(test_file),
            "--timeout=1",
            "-p",
            "no:cacheprovider",
            "-p",
            "no:cov",
            "-o",
            "asyncio_mode=auto",
        ],
        cwd=tmp_path,
        env=env,
        capture_output=True,
        text=True,
        timeout=50,
        check=False,
    )
    output = result.stdout + result.stderr
    assert result.returncode == 1, output
    assert "test_sleeps" in output
    assert "Timeout" in output
