import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


class WorkspaceProfileContextContract(unittest.TestCase):
    def test_template_setup_reuses_current_document_pack_as_context(self) -> None:
        source = read("src-tauri/src/subsystems/document_commands.rs")
        start = source.index("fn prepare_template_setup(")
        end = source.index(
            "#[derive(Debug, Deserialize)]\nstruct ImportLearningExampleFileRequest", start
        )
        command = source[start:end]

        self.assertIn("state: State<'_, AppState>", command)
        self.assertIn(
            'state.pack.lock().map_err(|_| "state lock failed")?.clone()', command
        )
        self.assertIn("prepare_template_confirmations_with_existing_pack", command)
        self.assertIn("Some(&pack)", command)

    def test_workspace_context_is_core_owned_not_a_second_tauri_classifier(self) -> None:
        core = read("crates/dokkomplekt-core/src/workspace_profile.rs")
        command = read("src-tauri/src/subsystems/document_commands.rs")

        self.assertIn("reinforce_workspace_inference_with_pack", core)
        self.assertIn("has_clear_conflict_with_existing_domain", core)
        self.assertNotIn("reinforce_workspace_inference_with_pack", command)


if __name__ == "__main__":
    unittest.main()
