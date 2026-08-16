from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one match, found {count}: {old[:140]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")

# useActionRunner intentionally returns undefined after a handled failure/cancel.
# Output policy already treats a missing plan as cancellation, so model that type honestly.
replace_once(
    "src/lib/outputFlow.ts",
    "getPlan: (root: string, parts: FolderNamePartDto[], labels: string[]) => Promise<PlannedOutput | null>;",
    "getPlan: (root: string, parts: FolderNamePartDto[], labels: string[]) => Promise<PlannedOutput | null | undefined>;",
)

# The scenario suite models an already configured returning user. The old implicit
# repository-relative fallback used to make this true accidentally. Set the real
# persisted folder explicitly instead of weakening the production fail-closed rule.
replace_once(
    "src/App.scenarios.test.tsx",
    "import { afterEach, describe, expect, it, vi } from 'vitest';",
    "import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';",
)
replace_once(
    "src/App.scenarios.test.tsx",
    "import { __resetInvokeForTests, __setInvokeForTests, rustCommandNames } from './lib/api';",
    "import { __resetInvokeForTests, __setInvokeForTests, rustCommandNames } from './lib/api';\nimport { OUTPUT_NAMING_CONFIRMED_KEY, OUTPUT_ROOT_KEY } from './lib/appSupport';",
)
replace_once(
    "src/App.scenarios.test.tsx",
    "describe('Полный прогон пользовательских сценариев и тем', () => {\n  afterEach(() => { __resetInvokeForTests(); vi.restoreAllMocks(); });",
    "describe('Полный прогон пользовательских сценариев и тем', () => {\n  beforeEach(() => {\n    localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Test/Готовые документы');\n    localStorage.setItem(OUTPUT_NAMING_CONFIRMED_KEY, '1');\n  });\n  afterEach(() => { localStorage.clear(); __resetInvokeForTests(); vi.restoreAllMocks(); });",
)

print("explicit output CI contracts aligned")
