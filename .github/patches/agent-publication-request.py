from pathlib import Path

storage_path = Path('crates/dokkomplekt-storage/src/lib.rs')
storage = storage_path.read_text(encoding='utf-8')
marker = '''pub struct TemplateVersionDraft {
    pub document_id: String,
    pub template_path: String,
    pub template_sha256: String,
    pub note: String,
}
'''
addition = marker + '''\npub struct DesktopSnapshotPublication<'a, T: ?Sized> {
    pub case_id: &'a str,
    pub pack_id: &'a str,
    pub case: &'a SemanticCase,
    pub pack: &'a DocumentPack,
    pub state_key: &'a str,
    pub state_value: &'a T,
    pub versions: &'a [TemplateVersionDraft],
}
'''
if storage.count(marker) != 1:
    raise SystemExit('TemplateVersionDraft marker mismatch')
storage = storage.replace(marker, addition, 1)
old_sig = '''    pub fn save_desktop_snapshot_with_template_versions<T: serde::Serialize + ?Sized>(
        &mut self,
        case_id: &str,
        pack_id: &str,
        case: &SemanticCase,
        pack: &DocumentPack,
        state_key: &str,
        state_value: &T,
        versions: &[TemplateVersionDraft],
    ) -> StorageResult<Vec<TemplateVersionRecord>> {
'''
new_sig = '''    pub fn save_desktop_snapshot_with_template_versions<T: serde::Serialize + ?Sized>(
        &mut self,
        publication: DesktopSnapshotPublication<'_, T>,
    ) -> StorageResult<Vec<TemplateVersionRecord>> {
        let DesktopSnapshotPublication {
            case_id,
            pack_id,
            case,
            pack,
            state_key,
            state_value,
            versions,
        } = publication;
'''
if storage.count(old_sig) != 1:
    raise SystemExit('publication method signature mismatch')
storage = storage.replace(old_sig, new_sig, 1)
# Rewrite the two storage regression calls.
old_call = '''            .save_desktop_snapshot_with_template_versions(
                "current",
                "default",
                &case,
                &candidate,
                "license_document",
                &Option::<String>::None,
                &[draft],
            )'''
new_call = '''            .save_desktop_snapshot_with_template_versions(DesktopSnapshotPublication {
                case_id: "current",
                pack_id: "default",
                case: &case,
                pack: &candidate,
                state_key: "license_document",
                state_value: &Option::<String>::None,
                versions: &[draft],
            })'''
if storage.count(old_call) != 1:
    raise SystemExit('valid test call mismatch')
storage = storage.replace(old_call, new_call, 1)
old_call2 = '''            .save_desktop_snapshot_with_template_versions(
                "current",
                "default",
                &case,
                &candidate,
                "license_document",
                &Option::<String>::None,
                &[invalid],
            )'''
new_call2 = '''            .save_desktop_snapshot_with_template_versions(DesktopSnapshotPublication {
                case_id: "current",
                pack_id: "default",
                case: &case,
                pack: &candidate,
                state_key: "license_document",
                state_value: &Option::<String>::None,
                versions: &[invalid],
            })'''
if storage.count(old_call2) != 1:
    raise SystemExit('invalid test call mismatch')
storage = storage.replace(old_call2, new_call2, 1)
storage_path.write_text(storage, encoding='utf-8')

main_path = Path('src-tauri/src/main.rs')
main = main_path.read_text(encoding='utf-8')
old_import = '''    CaseRunRecord, ClauseBlockRecord, CounterValue, LocalRepository, TemplateVersionDraft,
    TemplateVersionRecord, UsageReservation,
'''
new_import = '''    CaseRunRecord, ClauseBlockRecord, CounterValue, DesktopSnapshotPublication, LocalRepository,
    TemplateVersionDraft, TemplateVersionRecord, UsageReservation,
'''
if main.count(old_import) != 1:
    raise SystemExit('storage import mismatch')
main = main.replace(old_import, new_import, 1)
main_path.write_text(main, encoding='utf-8')

commands_path = Path('src-tauri/src/subsystems/document_commands.rs')
commands = commands_path.read_text(encoding='utf-8')
old = '''        .save_desktop_snapshot_with_template_versions(
            "current",
            "default",
            &case,
            &candidate,
            "license_document",
            &license,
            drafts,
        )'''
new = '''        .save_desktop_snapshot_with_template_versions(DesktopSnapshotPublication {
            case_id: "current",
            pack_id: "default",
            case: &case,
            pack: &candidate,
            state_key: "license_document",
            state_value: &license,
            versions: drafts,
        })'''
if commands.count(old) != 1:
    raise SystemExit('command publication call mismatch')
commands = commands.replace(old, new, 1)
commands_path.write_text(commands, encoding='utf-8')
