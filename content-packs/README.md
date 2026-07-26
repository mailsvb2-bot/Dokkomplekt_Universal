# Optional Tier-1 content-pack SDK

Эта папка не встраивает чужие нормативные тексты в универсальное ядро. Она задаёт проверяемый формат для отдельно поставляемых отраслевых пакетов: workflow, обязательные поля, DOCX/DOCM и их SHA-256.

## Статусы

- `workflow_skeleton` — только процесс и пустые слоты;
- `starter` — все слоты заполнены работающими **draft-only** DOCX-каркасами, но формы не утверждены организацией;
- `pilot` — все шаблоны проверены организацией на обезличенном корпусе и разрешены для пилота;
- `approved` — пакет опубликован ответственным владельцем и содержит именованных профильных рецензентов.

Три каталога `tier1-*` в этой версии имеют статус `starter`. Они позволяют сразу проверить импорт, разметку, popup и генерацию, но каждый документ содержит видимую оговорку и не должен использоваться как нормативная форма без профильного утверждения.

Проверка:

```bash
python scripts/validate_content_pack.py content-packs/tier1-hr-ru
python scripts/validate_content_pack.py content-packs/tier1-legal-ru
python scripts/validate_content_pack.py content-packs/tier1-accounting-ru
```

Воспроизводимое обновление starter-шаблонов:

```bash
python scripts/generate_starter_content_packs.py
```

Паки распространяются отдельно и не включаются в production-профиль без явного выбора пользователя и подтверждения статуса.

Production approval is detached and exact-revision-bound. See `APPROVAL_WORKFLOW.md`; bundled starter packs are not silently relabeled as legally approved.
