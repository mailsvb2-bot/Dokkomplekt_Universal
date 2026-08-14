from pathlib import Path

core = Path('crates/dokkomplekt-core/src/output_naming.rs')
text = core.read_text(encoding='utf-8')
text = text.replace('    ShortInitials,\n    OrganizationName,', '    ShortInitials,\n    SurnameGivenName,\n    OrganizationName,', 1)
text = text.replace('    PeriodRange,\n    // Backward-compatible', '    PeriodRange,\n    PeriodStartMonth,\n    PeriodEndMonth,\n    // Backward-compatible', 1)
short_arm = '''            FolderNamePart::ShortInitials => {
                if let Some(name) =
                    first(case, &["subject.name", "person.full_name", "patient.fio"])
                {
                    chunks.push(short_initials(name));
                }
            }
'''
if 'FolderNamePart::SurnameGivenName =>' not in text:
    text = text.replace(short_arm, short_arm + '''            FolderNamePart::SurnameGivenName => {
                if let Some(name) =
                    first(case, &["subject.name", "person.full_name", "patient.fio"])
                {
                    chunks.push(surname_given_name(name));
                }
            }
''', 1)
text = text.replace('''            FolderNamePart::AdmissionMonth => push(
                month_from_date(first(
                    case,
                    &["period.start_date", "medical.admission_date"],
                ))
                .as_deref(),
                &mut chunks,
            ),
            FolderNamePart::DischargeMonth => push(
''', '''            FolderNamePart::PeriodStartMonth | FolderNamePart::AdmissionMonth => push(
                month_from_date(first(
                    case,
                    &["period.start_date", "medical.admission_date"],
                ))
                .as_deref(),
                &mut chunks,
            ),
            FolderNamePart::PeriodEndMonth | FolderNamePart::DischargeMonth => push(
''', 1)
helper_anchor = '''fn month_from_date(value: Option<&str>) -> Option<String> {
'''
if 'fn surname_given_name' not in text:
    text = text.replace(helper_anchor, '''fn surname_given_name(name: &str) -> String {
    name.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
}
''' + helper_anchor, 1)
if 'surname_given_name_and_generic_months_preserve_old_folder_choices' not in text:
    test_anchor = '''    #[test]
    fn reserved_windows_names_and_trailing_dots_are_neutralized() {
'''
    test = '''    #[test]
    fn surname_given_name_and_generic_months_preserve_old_folder_choices() {
        let mut case = SemanticCase::default();
        for (id, value) in [
            ("subject.name", "Иванов Иван Иванович"),
            ("period.start_date", "01.06.2026"),
            ("period.end_date", "31.07.2026"),
        ] {
            case.values.insert(
                id.into(),
                SemanticValue::new(id, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        assert_eq!(
            build_output_folder_name(
                &case,
                &[
                    FolderNamePart::SurnameGivenName,
                    FolderNamePart::PeriodStartMonth,
                    FolderNamePart::PeriodEndMonth,
                ],
            ),
            "Иванов Иван 06.2026 07.2026"
        );
    }

'''
    text = text.replace(test_anchor, test + test_anchor, 1)
core.write_text(text, encoding='utf-8')

types = Path('src/lib/types.ts')
t = types.read_text(encoding='utf-8')
t = t.replace("  | 'ShortInitials'\n  | 'OrganizationName'", "  | 'ShortInitials'\n  | 'SurnameGivenName'\n  | 'OrganizationName'", 1)
t = t.replace("  | 'PeriodRange'\n  | 'AdmissionDate'", "  | 'PeriodRange'\n  | 'PeriodStartMonth'\n  | 'PeriodEndMonth'\n  | 'AdmissionDate'", 1)
types.write_text(t, encoding='utf-8')

ui = Path('src/components/UtilityPanel.tsx')
u = ui.read_text(encoding='utf-8')
u = u.replace("  { value: 'ShortInitials', label: 'фамилия и инициалы', sensitive: true },\n  { value: 'FullSubjectName'", "  { value: 'ShortInitials', label: 'фамилия и инициалы', sensitive: true },\n  { value: 'SurnameGivenName', label: 'фамилия и имя', sensitive: true },\n  { value: 'FullSubjectName'", 1)
u = u.replace("  { value: 'PeriodEndDate', label: 'окончание периода' },", "  { value: 'PeriodEndDate', label: 'окончание периода' },\n  { value: 'PeriodStartMonth', label: 'месяц начала периода' },\n  { value: 'PeriodEndMonth', label: 'месяц окончания периода' },", 1)
ui.write_text(u, encoding='utf-8')
