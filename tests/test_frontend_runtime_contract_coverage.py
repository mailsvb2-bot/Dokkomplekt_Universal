from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_every_tauri_call_has_exactly_one_fail_closed_response_contract() -> None:
    api = (ROOT / "src/lib/api.ts").read_text(encoding="utf-8")
    validation = (ROOT / "src/lib/runtimeValidation.ts").read_text(encoding="utf-8")

    api_commands = re.findall(r"callRust(?:<[^>]+>)?\(\s*'([^']+)'", api)
    registry_match = re.search(
        r"export const COMMAND_RESPONSE_KIND = \{(?P<body>.*?)\}\s+as const",
        validation,
        re.DOTALL,
    )
    assert registry_match is not None, "COMMAND_RESPONSE_KIND registry is missing"
    registry_commands = re.findall(
        r"^\s*'([^']+)'\s*:\s*'(?:array|boolean|string|void|nullable-object|object)'",
        registry_match.group("body"),
        re.MULTILINE,
    )

    assert len(registry_commands) == len(set(registry_commands)), "response registry contains duplicate commands"
    assert set(api_commands) == set(registry_commands), (
        f"missing={sorted(set(api_commands) - set(registry_commands))}; "
        f"stale={sorted(set(registry_commands) - set(api_commands))}"
    )


def test_runtime_validation_has_no_permissive_unknown_command_fallback() -> None:
    validation = (ROOT / "src/lib/runtimeValidation.ts").read_text(encoding="utf-8")
    assert "для команды не зарегистрирован контракт ответа" in validation
    assert "if (!kind)" in validation
