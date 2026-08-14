from pathlib import Path

path = Path('.agent_patch_general.py')
text = path.read_text(encoding='utf-8')
old = "        if in_literal and line.lstrip().startswith('required:'):\n"
new = "        if in_literal and (line.lstrip().startswith('required:') or line.strip() == 'required,'):\n"
assert text.count(old) == 1, text.count(old)
path.write_text(text.replace(old, new), encoding='utf-8')
print('PromptSpec literal migrator hardened')
