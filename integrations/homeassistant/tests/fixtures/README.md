# Fixtures of the integration's tests

`server.json`, `announce.json`, `announce-volume.json` and
`error-announce-origin.json` are the bytes of the three messages goal 18 adds
to the control plane (the server's identity, the `announce` command and its
`url` refusal), kept here until the server's change lands them under
`fixtures/control/v2/`; then these files go and the tests read the shared ones.

Everything that already exists is read from the repository's shared vectors,
`fixtures/control/` and `fixtures/control/v2/`, never copied here: the Python
client and the Rust server read the same files, so they cannot drift.
