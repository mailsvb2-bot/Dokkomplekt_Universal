from pathlib import Path

parent_path = Path("src-tauri/src/universal_intake.rs")
parent = parent_path.read_text(encoding="utf-8")
old_export = "pub use source_snapshot::{capture_stable_source, current_source_matches, StableSourceSnapshot};\n"
new_export = "pub use source_snapshot::{capture_stable_source, current_source_matches};\n"
if parent.count(old_export) != 1:
    raise SystemExit("source snapshot re-export marker mismatch")
parent = parent.replace(old_export, new_export, 1)
parent_path.write_text(parent, encoding="utf-8")

archive_path = Path("src-tauri/src/universal_intake/archive.rs")
archive = archive_path.read_text(encoding="utf-8")
archive_marker = "use super::*;\n"
if archive.count(archive_marker) != 1:
    raise SystemExit("archive import marker mismatch")
archive = archive.replace(archive_marker, "use super::*;\nuse std::io::Write as _;\n", 1)
archive_path.write_text(archive, encoding="utf-8")

web_path = Path("src-tauri/src/universal_intake/web.rs")
web = web_path.read_text(encoding="utf-8")
web_marker = "use super::*;\n"
if web.count(web_marker) != 1:
    raise SystemExit("web import marker mismatch")
web = web.replace(web_marker, "use super::*;\nuse sha2::{Digest as _, Sha256};\n", 1)
web_path.write_text(web, encoding="utf-8")
