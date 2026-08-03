#!/usr/bin/env bash
set -euo pipefail

watchdog_seconds="${DOKKOMPLEKT_LINUX_SMOKE_WATCHDOG_SECONDS:-180}"
if ! [[ "$watchdog_seconds" =~ ^[1-9][0-9]*$ ]]; then
  echo "DOKKOMPLEKT_LINUX_SMOKE_WATCHDOG_SECONDS must be a positive integer" >&2
  exit 1
fi
if [ "${DOKKOMPLEKT_LINUX_SMOKE_WATCHDOG_ACTIVE:-0}" != "1" ]; then
  command -v timeout >/dev/null || {
    echo "timeout is required for the Linux installer smoke watchdog" >&2
    exit 1
  }
  exec timeout --signal=TERM --kill-after=10s "${watchdog_seconds}s" \
    env DOKKOMPLEKT_LINUX_SMOKE_WATCHDOG_ACTIVE=1 bash "$0" "$@"
fi

bundle_dir="${1:-target/release/bundle}"
required_bundles="${DOKKOMPLEKT_REQUIRED_LINUX_BUNDLES:-appimage,deb,rpm}"
[ -d "$bundle_dir" ] || { echo "Bundle directory not found: $bundle_dir" >&2; exit 1; }
bundle_dir="$(cd "$bundle_dir" && pwd -P)"

appimage="$(find "$bundle_dir" -type f -name '*.AppImage' -print -quit)"
deb="$(find "$bundle_dir" -type f -name '*.deb' -print -quit)"
rpm_package="$(find "$bundle_dir" -type f -name '*.rpm' -print -quit)"

requires() { [[ ",${required_bundles}," == *",$1,"* ]]; }

cleanup_paths=()
cleanup_pids=()

process_is_running() {
  local pid="$1"
  local state
  state="$(ps -o stat= -p "$pid" 2>/dev/null | tr -d '[:space:]' || true)"
  [ -n "$state" ] && [[ "$state" != Z* ]]
}

stop_process_group() {
  local pid="$1"
  local pgid shell_pgid
  pgid="$(ps -o pgid= -p "$pid" 2>/dev/null | tr -d '[:space:]' || true)"
  shell_pgid="$(ps -o pgid= -p "$$" 2>/dev/null | tr -d '[:space:]' || true)"

  kill -TERM "$pid" 2>/dev/null || true
  if [ -n "$pgid" ] && [ "$pgid" != "$shell_pgid" ]; then
    kill -TERM -- "-$pgid" 2>/dev/null || true
  fi
  for _ in 1 2 3 4 5; do
    if ! process_is_running "$pid"; then
      wait "$pid" 2>/dev/null || true
      return 0
    fi
    sleep 0.2
  done

  kill -KILL "$pid" 2>/dev/null || true
  if [ -n "$pgid" ] && [ "$pgid" != "$shell_pgid" ]; then
    kill -KILL -- "-$pgid" 2>/dev/null || true
  fi
  for _ in 1 2 3 4 5; do
    if ! process_is_running "$pid"; then
      wait "$pid" 2>/dev/null || true
      return 0
    fi
    sleep 0.2
  done
  echo "WARNING: rendered smoke launcher did not terminate promptly: $pid" >&2
}

cleanup() {
  local pid path attempt
  for pid in "${cleanup_pids[@]:-}"; do
    [ -n "$pid" ] || continue
    stop_process_group "$pid"
  done
  for path in "${cleanup_paths[@]:-}"; do
    [ -n "$path" ] || continue
    for attempt in 1 2 3 4 5; do
      rm -rf -- "$path" 2>/dev/null || true
      [ ! -e "$path" ] && break
      sleep 0.2
    done
    if [ -e "$path" ]; then
      echo "WARNING: temporary smoke path remained after cleanup retries: $path" >&2
    fi
  done
}
trap cleanup EXIT

run_rendered_gui_smoke() {
  local executable="$1"
  local label="$2"
  local mode="$3"
  [ "${DOKKOMPLEKT_SKIP_LINUX_INSTALL_SMOKE:-0}" != "1" ] || return 0

  for command in xvfb-run dbus-run-session setsid timeout python ps tr; do
    command -v "$command" >/dev/null || {
      echo "$command is required for rendered $label smoke" >&2
      return 1
    }
  done

  local smoke_home display_file xauthority_file probe_error_file wrapper pid status
  local display xauthority evidence window_id captured_width captured_height colors
  local pixel_evidence_dir pixel_slug screenshot_file golden_report
  local probe_timeout_seconds
  probe_timeout_seconds="${DOKKOMPLEKT_X11_PROBE_TIMEOUT_SECONDS:-10}"
  if ! [[ "$probe_timeout_seconds" =~ ^[1-9][0-9]*$ ]]; then
    echo "DOKKOMPLEKT_X11_PROBE_TIMEOUT_SECONDS must be a positive integer" >&2
    return 1
  fi
  smoke_home="$(mktemp -d)"
  display_file="$smoke_home/display"
  xauthority_file="$smoke_home/xauthority-path"
  probe_error_file="$smoke_home/x11-probe.err"
  wrapper="$smoke_home/launch-wrapper.sh"
  pixel_evidence_dir="${DOKKOMPLEKT_PIXEL_EVIDENCE_DIR:-verification/ci}"
  pixel_slug="$(printf '%s' "$label" | tr -cs '[:alnum:]' '-' | tr '[:upper:]' '[:lower:]')"
  screenshot_file="$pixel_evidence_dir/webkit-${pixel_slug}.ppm"
  golden_report="$pixel_evidence_dir/webkit-${pixel_slug}-golden.json"
  mkdir -p "$pixel_evidence_dir"
  cleanup_paths+=("$smoke_home")
  cat >"$wrapper" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
display_file="$1"
xauthority_file="$2"
mode="$3"
executable="$4"
printf '%s\n' "$DISPLAY" >"$display_file"
printf '%s\n' "${XAUTHORITY:-}" >"$xauthority_file"
if [ "$mode" = "appimage" ]; then
  exec env APPIMAGE_EXTRACT_AND_RUN=1 "$executable"
fi
exec "$executable"
WRAPPER
  chmod +x "$wrapper"

  HOME="$smoke_home" XDG_CONFIG_HOME="$smoke_home/config" XDG_DATA_HOME="$smoke_home/data" \
    setsid xvfb-run -a -s '-screen 0 1280x960x24' dbus-run-session -- \
    "$wrapper" "$display_file" "$xauthority_file" "$mode" "$executable" >"$smoke_home/launch.log" 2>&1 &
  pid=$!
  cleanup_pids+=("$pid")

  for _ in $(seq 1 60); do
    if ! process_is_running "$pid"; then
      wait "$pid" || status=$?
      status="${status:-0}"
      cat "$smoke_home/launch.log" >&2
      echo "$label exited before rendering its window (code $status)" >&2
      return 1
    fi
    if [ -s "$display_file" ] && [ -s "$xauthority_file" ]; then
      display="$(cat "$display_file")"
      xauthority="$(cat "$xauthority_file")"
      if [ ! -r "$xauthority" ]; then
        sleep 0.25
        continue
      fi
      # xvfb-run creates a private MIT-MAGIC-COOKIE file and exports its path
      # only to descendants. The verifier runs in this parent shell, so it must
      # explicitly inherit that XAUTHORITY or XOpenDisplay cannot see any window.
      evidence="$(XAUTHORITY="$xauthority" timeout --signal=KILL "${probe_timeout_seconds}s" \
        python scripts/verify_rendered_x11_window.py \
        --display "$display" \
        --title "Dokkomplekt Universal" \
        --min-width 800 \
        --min-height 500 \
        --min-colors 64 \
        --screenshot "$screenshot_file" 2>"$probe_error_file" || true)"
      if [ -n "$evidence" ] \
        && grep -Fq 'Dokkomplekt native frontend IPC ready' "$smoke_home/launch.log"; then
        read -r window_id captured_width captured_height colors <<<"$evidence"
        if [[ "$window_id" =~ ^[0-9]+$ && "$captured_width" =~ ^[0-9]+$ \
          && "$captured_height" =~ ^[0-9]+$ && "$colors" =~ ^[0-9]+$ ]] \
          && python scripts/verify_webkit_pixel_golden.py \
            --image "$screenshot_file" \
            --baseline tests/fixtures/ui/webkit-linux-golden.json \
            --report "$golden_report"; then
          stop_process_group "$pid"
          printf -- '- %s packaged frontend IPC + pixel-golden smoke: OK (%sx%s, %s capturable X11 colors)\n' \
            "$label" "$captured_width" "$captured_height" "$colors"
          return 0
        fi
      fi
    fi
    sleep 0.25
  done

  cat "$smoke_home/launch.log" >&2
  if [ -s "$probe_error_file" ]; then
    echo "Last X11 probe error:" >&2
    cat "$probe_error_file" >&2
  fi
  stop_process_group "$pid"
  echo "$label did not prove native frontend IPC readiness in a mapped Dokkomplekt Universal X11 window within 15 seconds" >&2
  return 1
}

if requires appimage; then
  [ -n "$appimage" ] || { echo "AppImage not found" >&2; exit 1; }
fi
if requires deb; then
  [ -n "$deb" ] || { echo "deb package not found" >&2; exit 1; }
fi
if requires rpm; then
  [ -n "$rpm_package" ] || { echo "rpm package not found" >&2; exit 1; }
fi

if requires deb; then
  command -v dpkg-deb >/dev/null || { echo "dpkg-deb is required" >&2; exit 1; }
  dpkg-deb --info "$deb" >/dev/null
  deb_contents="$(dpkg-deb --contents "$deb")"
  grep -Eiq '/(usr/)?bin/[^/]*dokkomplekt|Dokkomplekt Universal' <<<"$deb_contents" || {
    echo "deb package does not contain the application executable" >&2
    exit 1
  }
fi

if requires rpm; then
  command -v rpm >/dev/null || { echo "rpm is required" >&2; exit 1; }
  rpm_contents="$(mktemp)"
  cleanup_paths+=("$rpm_contents")
  rpm -qpl "$rpm_package" >"$rpm_contents"
  grep -Eiq '/(usr/)?bin/[^/]*dokkomplekt|Dokkomplekt Universal' "$rpm_contents" || {
    echo "rpm package does not contain the application executable" >&2
    exit 1
  }
fi

if requires appimage; then
  extract_dir="$(mktemp -d)"
  cleanup_paths+=("$extract_dir")
  chmod +x "$appimage"
  (
    cd "$extract_dir"
    "$appimage" --appimage-extract >/dev/null
  )
  [ -f "$extract_dir/squashfs-root/AppRun" ] || {
    echo "AppImage extraction did not produce AppRun" >&2
    exit 1
  }
  runtime_source_manifest="$extract_dir/squashfs-root/usr/share/dokkomplekt/appimage-runtime.json"
  runtime_final_manifest="${appimage}.runtime-manifest.json"
  [ -s "$runtime_source_manifest" ] || {
    echo "AppImage graphics runtime source manifest is missing" >&2
    exit 1
  }
  [ -s "$runtime_final_manifest" ] || {
    echo "Final AppImage runtime integrity manifest is missing: $runtime_final_manifest" >&2
    exit 1
  }
  command -v python >/dev/null || { echo "python is required for AppImage runtime verification" >&2; exit 1; }
  python - "$appimage" "$extract_dir/squashfs-root/usr/lib" "$runtime_source_manifest" "$runtime_final_manifest" <<'PY'
import hashlib
import json
import platform
import sys
from pathlib import Path

appimage_path = Path(sys.argv[1])
lib_root = Path(sys.argv[2])
source_manifest_path = Path(sys.argv[3])
final_manifest_path = Path(sys.argv[4])
required = ("libGLESv2.so.2", "libEGL.so.1", "libGLdispatch.so.0")
arch = {"x86_64": ("x86_64", 62), "amd64": ("x86_64", 62), "aarch64": ("aarch64", 183), "arm64": ("aarch64", 183)}.get(platform.machine().lower())
if arch is None:
    raise SystemExit(f"unsupported smoke architecture: {platform.machine()}")
arch_name, expected_machine = arch


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


source_raw = source_manifest_path.read_bytes()
source_manifest = json.loads(source_raw.decode("utf-8"))
if source_manifest.get("schema") != 2 or source_manifest.get("phase") != "pre-linuxdeploy":
    raise SystemExit("unsupported AppImage runtime source manifest")
if source_manifest.get("targetArch") != arch_name:
    raise SystemExit("AppImage runtime source manifest architecture mismatch")
source_records = {entry.get("name"): entry for entry in source_manifest.get("libraries", []) if isinstance(entry, dict)}

final_manifest = json.loads(final_manifest_path.read_text("utf-8"))
if final_manifest.get("schema") != 1 or final_manifest.get("phase") != "post-linuxdeploy":
    raise SystemExit("unsupported final AppImage runtime manifest")
if final_manifest.get("targetArch") != arch_name:
    raise SystemExit("final AppImage runtime manifest architecture mismatch")
if final_manifest.get("embeddedSourceManifestSha256") != hashlib.sha256(source_raw).hexdigest():
    raise SystemExit("final AppImage manifest references the wrong embedded source manifest")
appimage_record = final_manifest.get("appImage")
if not isinstance(appimage_record, dict):
    raise SystemExit("final runtime manifest has no AppImage record")
if appimage_record.get("name") != appimage_path.name:
    raise SystemExit("final runtime manifest names the wrong AppImage")
if appimage_record.get("size") != appimage_path.stat().st_size:
    raise SystemExit("final runtime manifest AppImage size mismatch")
if appimage_record.get("sha256") != sha256_file(appimage_path):
    raise SystemExit("final runtime manifest AppImage SHA-256 mismatch")
final_records = {entry.get("name"): entry for entry in final_manifest.get("libraries", []) if isinstance(entry, dict)}

for name in required:
    path = lib_root / name
    if not path.is_file() or path.stat().st_size == 0:
        raise SystemExit(f"AppImage is missing required graphics runtime: {name}")
    data = path.read_bytes()
    if len(data) < 20 or data[:4] != b"\x7fELF" or data[4] != 2:
        raise SystemExit(f"AppImage graphics runtime is not a 64-bit ELF binary: {name}")
    byteorder = "little" if data[5] == 1 else "big" if data[5] == 2 else None
    if byteorder is None:
        raise SystemExit(f"AppImage graphics runtime has invalid ELF byte order: {name}")
    machine = int.from_bytes(data[18:20], byteorder)
    if machine != expected_machine:
        raise SystemExit(f"AppImage graphics runtime has wrong architecture: {name} ({machine})")

    source = source_records.get(name)
    if not isinstance(source, dict):
        raise SystemExit(f"AppImage runtime source manifest does not describe: {name}")
    source_size = source.get("sourceSize")
    source_sha256 = source.get("sourceSha256")
    if not isinstance(source_size, int) or source_size <= 0:
        raise SystemExit(f"AppImage runtime source manifest has invalid size: {name}")
    if not isinstance(source_sha256, str) or len(source_sha256) != 64:
        raise SystemExit(f"AppImage runtime source manifest has invalid SHA-256: {name}")
    if source.get("elfMachine") != machine:
        raise SystemExit(f"AppImage runtime source manifest architecture mismatch: {name}")

    final = final_records.get(name)
    if not isinstance(final, dict):
        raise SystemExit(f"final AppImage runtime manifest does not describe: {name}")
    digest = hashlib.sha256(data).hexdigest()
    if final.get("sha256") != digest or final.get("size") != len(data) or final.get("elfMachine") != machine:
        raise SystemExit(f"final AppImage runtime integrity mismatch: {name}")
    if final.get("sourceSize") != source_size or final.get("sourceSha256") != source_sha256:
        raise SystemExit(f"final AppImage runtime provenance mismatch: {name}")
PY
  executable_payload="$(find "$extract_dir/squashfs-root" -type f -perm -u+x \
    \( -name 'AppRun' -o -iname '*dokkomplekt*' \) -print -quit)"
  [ -n "$executable_payload" ] || {
    echo "AppImage does not contain an executable application payload" >&2
    exit 1
  }
  run_rendered_gui_smoke "$appimage" "AppImage" "appimage"
fi

run_deb_install_smoke() {
  [ "${DOKKOMPLEKT_SKIP_LINUX_INSTALL_SMOKE:-0}" != "1" ] || return 0
  command -v dpkg >/dev/null || { echo "dpkg is required for install smoke" >&2; return 1; }

  local package_name install_log remove_log binary_path package_files
  package_name="$(dpkg-deb -f "$deb" Package)"
  install_log="$(mktemp)"
  remove_log="$(mktemp)"
  cleanup_paths+=("$install_log" "$remove_log")

  local -a privilege=()
  if [ "$(id -u)" -ne 0 ]; then
    command -v sudo >/dev/null || { echo "sudo is required for deb install smoke" >&2; return 1; }
    privilege=(sudo)
  fi

  cleanup_deb_install() {
    if dpkg-query -W -f='${Status}' "$package_name" 2>/dev/null | grep -q 'install ok installed'; then
      "${privilege[@]}" dpkg --remove "$package_name" >"$remove_log" 2>&1 || true
    fi
  }

  cleanup_deb_install
  if ! "${privilege[@]}" dpkg --install "$deb" >"$install_log" 2>&1; then
    cat "$install_log" >&2
    echo "deb install smoke failed" >&2
    return 1
  fi

  package_files="$(dpkg-query -L "$package_name")"
  binary_path="$(awk '/\/(usr\/)?bin\/[^/]*dokkomplekt/ { print; exit }' <<<"$package_files")"
  if [ -z "$binary_path" ] || [ ! -x "$binary_path" ]; then
    echo "installed executable was not found for $package_name" >&2
    cleanup_deb_install
    return 1
  fi

  if ! run_rendered_gui_smoke "$binary_path" "installed application" "binary"; then
    cleanup_deb_install
    return 1
  fi

  cleanup_deb_install
  if dpkg-query -W -f='${Status}' "$package_name" 2>/dev/null | grep -q 'install ok installed'; then
    cat "$remove_log" >&2
    echo "deb uninstall smoke did not remove $package_name" >&2
    return 1
  fi

  printf -- '- deb install/render/uninstall smoke: OK (%s)\n' "$package_name"
}

if requires deb; then
  run_deb_install_smoke
fi

printf 'Linux bundle validation OK (required: %s)\n' "$required_bundles"
[ -z "$appimage" ] || printf -- '- AppImage: %s\n' "$appimage"
[ -z "$deb" ] || printf -- '- deb: %s\n' "$deb"
[ -z "$rpm_package" ] || printf -- '- rpm: %s\n' "$rpm_package"
