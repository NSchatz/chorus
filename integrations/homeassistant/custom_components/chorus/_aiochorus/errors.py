"""What can go wrong talking to a chorus server."""

from __future__ import annotations


class ChorusError(Exception):
    """Base of every error this client raises."""


class ChorusConnectionError(ChorusError):
    """The server could not be reached, or stopped answering."""


class ChorusProtocolError(ChorusError):
    """The server answered with something the control plane does not declare."""


class ChorusUnsupportedError(ChorusError):
    """The server does not offer what this client needs (catalog version 2)."""


class ChorusCommandError(ChorusError):
    """The server answered a command with an ``error`` naming a field (HTTP 400).

    ``field`` is the contract; ``detail`` is wording for a person and is never
    matched.
    """

    def __init__(self, field: str, detail: str) -> None:
        """Keep the field the server named and its wording."""
        super().__init__(f"the server refused the command (field '{field}'): {detail}")
        self.field = field
        self.detail = detail

    @property
    def name(self) -> str | None:
        """Return the refusal's name, when the detail starts with one.

        The catalog's later refusals start their detail with a name, a colon
        and a space (``busy: speaker ...``): the name is the contract, the words
        after it are not.
        """
        head, colon, _ = self.detail.partition(": ")
        if colon and head and all(c.islower() or c == "-" for c in head):
            return head
        return None


class ChorusRefusedError(ChorusUnsupportedError):
    """The server answered ``refused`` (HTTP 426): the catalog version is not one it has."""

    def __init__(
        self, detail: str, offered: int | None, implemented: tuple[int, ...]
    ) -> None:
        """Keep what was offered and what the server implements."""
        super().__init__(detail)
        self.detail = detail
        self.offered = offered
        self.implemented = implemented
