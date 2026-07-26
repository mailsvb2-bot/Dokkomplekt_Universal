# Known external limitations

The source checkpoint is not represented as a completed signed production release.

Not supplied or not proven in this environment:

- a signed NSIS installer and Authenticode certificate;
- clean-Windows installation, reboot, watcher-after-logon, licensed Word COM, PrintService and physical/virtual printer evidence;
- trusted bundled Tesseract, Poppler, LibreOffice, 7-Zip, MSG converter, SumatraPDF, local LLM runtime and model weights;
- real two-machine PostgreSQL/shared-folder acceptance evidence;
- approved professional normative packs and measured accuracy on real anonymized corpora;
- certified PDF/A, qualified electronic signature and deployment-specific 152-FZ documentation;
- production credentials and contracts for specific CRM, EDI and medical-information-system integrations;
- guaranteed recognition of handwriting, CAD/database files, encrypted/protected files or arbitrary unknown formats.

The runtime intentionally remains fail-closed while `sidecar-status.json` reports `ready=false`.
