# Private Windows release trust boundary

The audited workflow in this directory must keep production signing/runtime work on `dokkomplekt-runtime` and Word/printer/reboot evidence on the physically distinct `dokkomplekt-hardware` runner. The signed handoff binds both host fingerprints and fails closed when they match. Full operational instructions live in `docs/WINDOWS_HARDWARE_RUNNER.md`.
