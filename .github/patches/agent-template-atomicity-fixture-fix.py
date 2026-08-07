from pathlib import Path

path = Path("crates/dokkomplekt-storage/src/lib.rs")
text = path.read_text(encoding="utf-8")
needle = '            pack_name: "'
count = text.count(needle)
if count != 4:
    raise SystemExit(f"expected exactly four temporary test fixture fields, got {count}")
text = text.replace(needle, '            name: "')
path.write_text(text, encoding="utf-8")
