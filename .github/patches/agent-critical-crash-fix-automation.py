from pathlib import Path

path = Path("src-tauri/src/subsystems/automation_runtime.rs")
text = path.read_text(encoding="utf-8")

replacements = [
    (
        "    let target = reservation.commit();\n    Ok(ImportTemplateFileResponse {",
        "    let target = reservation.commit()?;\n    Ok(ImportTemplateFileResponse {",
    ),
    (
        "            Ok(reservation.commit())\n        })();",
        "            reservation.commit()\n        })();",
    ),
    (
        "            let destination = reservation.commit();\n            let (size_bytes, _, sha256) = file_content_signature(&destination)?;",
        "            let destination = reservation.commit()?;\n            let (size_bytes, _, sha256) = file_content_signature(&destination)?;",
    ),
]

for old, new in replacements:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match, got {count}: {old!r}")
    text = text.replace(old, new, 1)

path.write_text(text, encoding="utf-8")
