# Contributing

## Principles

Dokkomplekt keeps business behaviour in Rust. React/TypeScript is a thin desktop UI and must not duplicate parser, workflow, licensing, validation or document-generation decisions.

Changes must preserve these invariants:

1. no bundled professional document content;
2. templates teach structure, not patient/customer meaning;
3. required popup values fail closed and invalid input keeps the modal open;
4. desktop trust anchors cannot be supplied by UI;
5. one source event cannot create duplicate UI/background processing;
6. release packaging is blocked until real Cargo and platform installer gates pass.

## Local checks

```bash
npm ci
npm run typecheck
npm run test
npm run build
python scripts/run_python_contracts_sharded.py --report verification/local/python-contracts.json
python scripts/static_quality_gate.py
bash scripts/prepackage_rust_gate.sh
python scripts/assert_release_ready.py
```

On Windows, also run `tests/installer/windows_installer_contract.ps1`. Word COM tests require Microsoft Word. On Linux, run `tests/installer/linux_installer_contract.sh` after building the bundles.

## Pull requests

Keep changes focused, add regression tests for each fixed defect class, update `CHANGELOG.md` and the release verification report, and do not commit `node_modules`, `target`, build output, private keys, `.env`, generated installers or personal/professional documents.
