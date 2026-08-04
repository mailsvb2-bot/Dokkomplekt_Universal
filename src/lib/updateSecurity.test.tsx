import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { UtilityPanel } from "../components/UtilityPanel";
import {
  __resetInvokeForTests,
  __setInvokeForTests,
  checkForUpdates,
  rustCommandNames,
} from "./api";

const updateResult = {
  available: true,
  current_version: "18.0.7",
  latest_version: "18.0.8",
  platform: "windows-x86_64",
  message: "verified",
  notes: "security update",
  verified_package_path: "C:/updates/dokkomplekt.exe",
  sha256: "a".repeat(64),
  size_bytes: 42,
};

describe("signed update UI contract", () => {
  afterEach(() => __resetInvokeForTests());

  it("invokes the fixed Rust update command without UI trust parameters", async () => {
    const invoke = vi.fn(async () => updateResult as never);
    __setInvokeForTests(invoke);
    await expect(checkForUpdates()).resolves.toEqual(updateResult);
    expect(invoke).toHaveBeenCalledWith("check_for_updates", undefined);
  });

  it("registers the update command exactly once", () => {
    expect(
      rustCommandNames.filter((name) => name === "check_for_updates"),
    ).toHaveLength(1);
  });

  it("keeps the verified package metadata in the typed response", async () => {
    __setInvokeForTests(async () => updateResult as never);
    const result = await checkForUpdates();
    expect(result).toMatchObject({
      available: true,
      latest_version: "18.0.8",
      size_bytes: 42,
    });
    expect(result.sha256).toHaveLength(64);
  });

  it("shows a visible Check updates button", async () => {
    await renderUtility();
    expect(
      screen.getByRole("button", { name: "Проверить обновления" }),
    ).toBeTruthy();
  });

  it("routes the visible button to its callback", async () => {
    const onCheckUpdates = vi.fn();
    await renderUtility(onCheckUpdates);
    fireEvent.click(
      screen.getByRole("button", { name: "Проверить обновления" }),
    );
    expect(onCheckUpdates).toHaveBeenCalledTimes(1);
  });

  it("does not render manifest URL or public-key fields", async () => {
    await renderUtility();
    expect(screen.queryByPlaceholderText(/manifest/i)).toBeNull();
    expect(screen.queryByPlaceholderText(/public key/i)).toBeNull();
  });
});

async function renderUtility(onCheckUpdates = vi.fn()) {
  __setInvokeForTests(async (command) => {
    switch (command) {
      case "get_privacy_preferences":
        return {
          copy_source_to_output: true,
          write_trust_report: true,
          include_values_in_trust_report: false,
          temp_retention_hours: 24,
        } as never;
      case "list_automation_exceptions":
      case "list_audit_events":
        return [] as never;
      case "get_queue_status": return { mode: "shared_filesystem", configured: false, reachable: true, message: "ok" } as never;
      case "get_corpus_status": return { recording_enabled: false, entry_count: 0, privacy_mode: "encrypted-hashed-no-raw-values", message: "off" } as never;
      case "get_automation_metrics":
        return {
          processed_sources: 0,
          generated_documents: 0,
          blocked_sources: 0,
          failed_sources: 0,
          print_failures: 0,
          user_confirmations: 0,
        } as never;
      default:
        throw new Error(`Unexpected command in UtilityPanel test: ${command}`);
    }
  });
  render(
    <UtilityPanel
      documents={[]}
      selectedDocumentIds={[]}
      onStatus={vi.fn()}
      onDocumentsChanged={vi.fn()}
      seriesStart=""
      seriesEnd=""
      seriesSkipWeekends={false}
      scannerField=""
      scannerText=""
      outputRoot="output"
      folderParts={[]}
      licenseText=""
      onSeriesStartChange={vi.fn()}
      onSeriesEndChange={vi.fn()}
      onSeriesSkipWeekendsChange={vi.fn()}
      onScannerFieldChange={vi.fn()}
      onScannerTextChange={vi.fn()}
      onOutputRootChange={vi.fn()}
      onPickOutputFolder={vi.fn()}
      onFolderPartsChange={vi.fn()}
      onLicenseTextChange={vi.fn()}
      onSeriesPlan={vi.fn()}
      onScanMarks={vi.fn()}
      onOutputPlan={vi.fn()}
      onSaveSession={vi.fn()}
      onLoadSession={vi.fn()}
      onCheckAccess={vi.fn()}
      onCheckUpdates={onCheckUpdates}
      onInstallWatcher={vi.fn()}
      onUninstallWatcher={vi.fn()}
      onVerifyLicense={vi.fn()}
    />,
  );
  await screen.findByText("Неразрешённых остановок нет.");
}
