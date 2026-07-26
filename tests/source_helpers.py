from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def project_text(relative: str) -> str:
    path = ROOT / relative
    text = path.read_text(encoding="utf-8")
    if relative == "src-tauri/src/main.rs":
        subsystem_root = ROOT / "src-tauri/src/subsystems"
        text += "\n" + "\n".join(
            child.read_text(encoding="utf-8")
            for child in sorted(subsystem_root.glob("*.rs"))
        )
    return text
