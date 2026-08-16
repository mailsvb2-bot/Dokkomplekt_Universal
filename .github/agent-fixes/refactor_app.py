from pathlib import Path
import re

app = Path("src/App.tsx")
text = app.read_text(encoding="utf-8")

text = text.replace(
    "import type { CreatedDocumentsIntakeResult, GeneratedOutput, GeneratedPrintItem, IntakeCapability, SidecarToolStatus, PrintJobDto, PrintTriageReport, SemanticExtractResult, DocumentRoutingRecommendation, DocumentTemplateSpec, DomainKind, FolderNamePartDto, Icd10Suggestion, LearnedScannerRule, PopupFieldConfig, WorkflowPlan, SupplementarySourceDto, OutputConflictPolicy } from './lib/types';",
    "import type { CreatedDocumentsIntakeResult, GeneratedOutput, GeneratedPrintItem, IntakeCapability, SidecarToolStatus, PrintJobDto, PrintTriageReport, SemanticExtractResult, DocumentRoutingRecommendation, DocumentTemplateSpec, DomainKind, FolderNamePartDto, Icd10Suggestion, LearnedScannerRule, PopupFieldConfig, WorkflowPlan } from './lib/types';",
    1,
)
text = text.replace(
    "  checkForUpdates, pickFolder, pickTemplateFiles, validateProductAccess, verifyRustLicenseText, attachSupplementaryFile, attachSupplementaryFolder, listSupplementarySources, removeSupplementarySource,\n",
    "  checkForUpdates, pickFolder, pickTemplateFiles, validateProductAccess, verifyRustLicenseText,\n",
    1,
)
text = text.replace(
    "import { useGenerationPreflight } from './hooks/useGenerationPreflight';\n",
    "import { useGenerationPreflight } from './hooks/useGenerationPreflight';\nimport { useOutputSupplementaryFlow } from './hooks/useOutputSupplementaryFlow';\n",
    1,
)
for name in [
    "OUTPUT_NAMING_PRESETS, ",
    "loadOutputFolderParts, ",
    "loadOutputNamingConfirmed, ",
    "outputNamingPreset, ",
    "saveOutputFolderParts, ",
]:
    text = text.replace(name, "", 1)

for line in [
    "  const [folderParts, setFolderParts] = useState<FolderNamePartDto[]>(loadOutputFolderParts);\n",
    "  const [outputNamingConfirmed, setOutputNamingConfirmed] = useState(loadOutputNamingConfirmed);\n",
    "  const [supplementarySources, setSupplementarySources] = useState<SupplementarySourceDto[]>([]);\n",
]:
    if text.count(line) != 1:
        raise SystemExit(f"state line not unique: {line.strip()} count={text.count(line)}")
    text = text.replace(line, "", 1)

effect_pattern = re.compile(
    r"\n  useEffect\(\(\) => \{\n"
    r"    let alive = true;\n"
    r"    void listSupplementarySources\(\)\n"
    r"      \.then\(\(response\) => \{ if \(alive\) setSupplementarySources\(response\.sources\); \}\)\n"
    r"      \.catch\(\(\) => \{ /\* browser/tests \*/ \}\);\n"
    r"    return \(\) => \{ alive = false; \};\n"
    r"  \}, \[\]\);\n"
)
text, count = effect_pattern.subn("\n", text, count=1)
if count != 1:
    raise SystemExit(f"supplementary bootstrap effect count={count}")

anchor = "  const [guidedScanner, setGuidedScanner] = useState<GuidedScannerState | null>(null);\n"
hook_call = """  const [guidedScanner, setGuidedScanner] = useState<GuidedScannerState | null>(null);

  const {
    folderParts,
    supplementarySources,
    setSupplementarySources,
    updateFolderParts,
    ensureOutputNamingConfirmed,
    attachSupplementaryFiles,
    attachSupplementaryFolderByRole,
    removeSupplementary,
    outputConflictPolicy,
  } = useOutputSupplementaryFlow({
    dialogs,
    run,
    documents,
    outputRoot,
    ensureComponentForSource,
    setUtilityOpen,
    setStatus,
  });
"""
if text.count(anchor) != 1:
    raise SystemExit("guided scanner anchor not unique")
text = text.replace(anchor, hook_call, 1)

update_parts = re.compile(
    r"\n  function updateFolderParts\(parts: FolderNamePartDto\[\]\) \{\n"
    r".*?\n  \}\n\n  function updateAutoPrint",
    re.S,
)
text, count = update_parts.subn("\n  function updateAutoPrint", text, count=1)
if count != 1:
    raise SystemExit(f"updateFolderParts block count={count}")

flow_block = re.compile(
    r"\n  async function ensureOutputNamingConfirmed\(\): Promise<FolderNamePartDto\[\] \| null> \{"
    r".*?"
    r"\n  async function performGenerateSelectedDocuments\(documentIds: string\[\]\) \{",
    re.S,
)
text, count = flow_block.subn(
    "\n  async function performGenerateSelectedDocuments(documentIds: string[]) {",
    text,
    count=1,
)
if count != 1:
    raise SystemExit(f"moved flow block count={count}")

app.write_text(text, encoding="utf-8")
