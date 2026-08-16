from pathlib import Path

path = Path('.agent_apply_donor_contracts.py')
lines = path.read_text(encoding='utf-8').splitlines()
start_count = 0
end_count = 0
for index, line in enumerate(lines):
    if line == '    """        "docx"':
        lines[index] = "    '''        \"docx\""
        start_count += 1
    if line == '            | "pdf"""",':
        lines[index] = "            | \"pdf\"''',"
        end_count += 1
if start_count != 2 or end_count != 2:
    raise SystemExit(f'unexpected quoting repair counts: starts={start_count}, ends={end_count}')
path.write_text('\n'.join(lines) + '\n', encoding='utf-8')
