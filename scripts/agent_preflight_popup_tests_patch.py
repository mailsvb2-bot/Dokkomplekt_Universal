from pathlib import Path

path = Path('src/App.scenarios.test.tsx')
text = path.read_text(encoding='utf-8')

old_workflow = "const workflow = { document_id: 'acc_1', prompts: [{ field_id: 'org.inn', title: 'ИНН', required: true, current_value: '7701234567', validation_hint: null }], blocked: false, block_reasons: [] };"
new_workflow = "const workflow = { document_id: 'acc_1', prompts: [{ field_id: 'org.inn', title: 'ИНН', required: true, skippable: true, current_value: '7701234567', validation_hint: null }], blocked: false, block_reasons: [] };"
assert text.count(old_workflow) == 1
text = text.replace(old_workflow, new_workflow, 1)

old_batch = '''    // One action applies the visible answers and creates the selected package.
    await click(/Проверить и создать \\(2\\)/);
    await waitFor(() => expect(parsePayload(calls, 'apply_popup_batch')).toMatchObject({
      req: { document_ids: ['acc_1', 'doc_2'], answers: [{ field_id: 'org.inn', value: '7701234567' }] },
    }));
'''
new_batch = '''    // Creation is deliberately two-step: the workspace action opens a blocking preflight,
    // and no Rust apply/render call happens until the user confirms that modal.
    const batchApplyCountBefore = calls.filter((call) => call.command === 'apply_popup_batch').length;
    await click(/Проверить и создать \\(2\\)/);
    const batchPreflight = await screen.findByRole('dialog', { name: 'Проверка перед созданием' });
    expect(calls.filter((call) => call.command === 'apply_popup_batch')).toHaveLength(batchApplyCountBefore);
    fireEvent.click(within(batchPreflight).getByRole('button', { name: 'Создать документы' }));
    await waitFor(() => expect(parsePayload(calls, 'apply_popup_batch')).toMatchObject({
      req: { document_ids: ['acc_1', 'doc_2'], answers: [{ field_id: 'org.inn', value: '7701234567' }] },
    }));
'''
assert text.count(old_batch) == 1
text = text.replace(old_batch, new_batch, 1)

old_single = '''    await click(/Проверить и создать \\(1\\)/);
    await waitFor(() => expect(parsePayload(calls, 'apply_popup')).toMatchObject({
      req: { document_id: 'acc_1', answers: [{ field_id: 'org.inn', value: '7701234567' }] },
    }));
'''
new_single = '''    await click(/Проверить и создать \\(1\\)/);
    const singlePreflight = await screen.findByRole('dialog', { name: 'Проверка перед созданием' });
    fireEvent.click(within(singlePreflight).getByRole('button', { name: 'Создать документы' }));
    await waitFor(() => expect(parsePayload(calls, 'apply_popup')).toMatchObject({
      req: { document_id: 'acc_1', answers: [{ field_id: 'org.inn', value: '7701234567' }] },
    }));
'''
assert text.count(old_single) == 1
text = text.replace(old_single, new_single, 1)

old_skip = '''    await click(/Проверить и создать \\(2\\)/);
    await waitFor(() => expect(parsePayload(calls, 'apply_popup_batch')).toMatchObject({
      req: {
        document_ids: ['acc_1', 'doc_2'],
        answers: [{ field_id: 'org.inn', value: '', continue_without_value: true }],
      },
    }));
'''
new_skip = '''    await click(/Проверить и создать \\(2\\)/);
    const preflight = await screen.findByRole('dialog', { name: 'Проверка перед созданием' });
    expect(within(preflight).getByRole('button', { name: 'Вернуться к заполнению' }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(within(preflight).getByRole('button', { name: 'Создать документы' }));
    await waitFor(() => expect(parsePayload(calls, 'apply_popup_batch')).toMatchObject({
      req: {
        document_ids: ['acc_1', 'doc_2'],
        answers: [{ field_id: 'org.inn', value: '', continue_without_value: true }],
      },
    }));
'''
assert text.count(old_skip) == 1
text = text.replace(old_skip, new_skip, 1)

path.write_text(text, encoding='utf-8')
