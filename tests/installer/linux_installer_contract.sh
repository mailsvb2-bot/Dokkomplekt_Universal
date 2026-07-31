#!/usr/bin/env bash
set -euo pipefail

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

stop_process_group() {
  local pid="$1"
  kill -TERM -- "-$pid" 2>/dev/null || true
  for _ in 1 2 3 4 5; do
    kill -0 -- "-$pid" 2>/dev/null || break
    sleep 0.2
  done
  kill -KILL -- "-$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
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

  for command in xvfb-run dbus-run-session setsid xwininfo import identify; do
    command -v "$command" >/dev/null || {
      echo "$command is required for rendered $label smoke" >&2
      return 1
    }
  done

  local smoke_home display_file wrapper pid status display window_id info
  local x y width height screenshot captured_width captured_height colors metrics
  smoke_home="$(mktemp -d)"
  display_file="$smoke_home/display"
  wrapper="$smoke_home/launch-wrapper.sh"
  screenshot="$smoke_home/window.png"
  cleanup_paths+=("$smoke_home")
  cat >"$wrapper" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
display_file="$1"
mode="$2"
executable="$3"
printf '%s\n' "$DISPLAY" >"$display_file"
if [ "$mode" = "appimage" ]; then
  exec env APPIMAGE_EXTRACT_AND_RUN=1 "$executable"
fi
exec "$executable"
WRAPPER
  chmod +x "$wrapper"

  HOME="$smoke_home" XDG_CONFIG_HOME="$smoke_home/config" XDG_DATA_HOME="$smoke_home/data" \
    setsid xvfb-run -a -s '-screen 0 1280x960x24' dbus-run-session -- \
    "$wrapper" "$display_file" "$mode" "$executable" >"$smoke_home/launch.log" 2>&1 &
  pid=$!
  cleanup_pids+=("$pid")

  for _ in $(seq 1 60); do
    if ! kill -0 -- "-$pid" 2>/dev/null; then
      wait "$pid" || status=$?
      status="${status:-0}"
      cat "$smoke_home/launch.log" >&2
      echo "$label exited before rendering its window (code $status)" >&2
      return 1
    fi
    if [ -s "$display_file" ]; then
      display="$(cat "$display_file")"
      window_id="$(DISPLAY="$display" xwininfo -root -tree 2>/dev/null | awk '/"Dokkomplekt Universal"/ { print $1; exit }' || true)"
      if [ -n "$window_id" ]; then
        info="$(DISPLAY="$display" xwininfo -id "$window_id" 2>/dev/null || true)"
        x="$(awk -F: '/Absolute upper-left X/ { gsub(/ /, "", $2); print $2; exit }' <<<"$info")"
        y="$(awk -F: '/Absolute upper-left Y/ { gsub(/ /, "", $2); print $2; exit }' <<<"$info")"
        width="$(awk -F: '/Width/ { gsub(/ /, "", $2); print $2; exit }' <<<"$info")"
        height="$(awk -F: '/Height/ { gsub(/ /, "", $2); print $2; exit }' <<<"$info")"
        if [[ "$x" =~ ^-?[0-9]+$ && "$y" =~ ^-?[0-9]+$ && "$width" =~ ^[0-9]+$ && "$height" =~ ^[0-9]+$ ]]; then
          if DISPLAY="$display" import -silent -window "$window_id" "$screenshot" >/dev/null 2>&1; then
            metrics="$(identify -format '%w %h %k\n' "$screenshot" 2>/dev/null || true)"
            read -r captured_width captured_height colors <<<"$metrics"
            if [[ "$captured_width" =~ ^[0-9]+$ && "$captured_height" =~ ^[0-9]+$ && "$colors" =~ ^[0-9]+$ ]] \
              && [ "$captured_width" -ge 800 ] && [ "$captured_height" -ge 500 ] && [ "$colors" -ge 64 ]; then
              stop_process_group "$pid"
              printf -- '- %s rendered GUI smoke: OK (%sx%s, %s colors)\n' \
                "$label" "$captured_width" "$captured_height" "$colors"
              return 0
            fi
          fi
        fi
      fi
    fi
    sleep 0.25
  done

  cat "$smoke_home/launch.log" >&2
  if [ -f "$screenshot" ]; then
    identify "$screenshot" >&2 || true
  fi
  stop_process_group "$pid"
  echo "$label did not render a non-blank Dokkomplekt Universal window within 15 seconds" >&2
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
  runtime_manifest="$extract_dir/squashfs-root/usr/share/dokkomplekt/appimage-runtime.json"
  [ -s "$runtime_manifest" ] || {
    echo "AppImage graphics runtime manifest is missing" >&2
    exit 1
  }
  command -v python >/dev/null || { echo "python is required for AppImage runtime verification" >&2; exit 1; }
  python - "$extract_dir/squashfs-root/usr/lib" "$runtime_manifest" <<'PY'
import hashlib
import json
import platform
import sys
from pathlib import Path

lib_root = Path(sys.argv[1])
manifest_path = Path(sys.argv[2])
expected_machine = {"x86_64": 62, "aarch64": 183}.get(platform.machine().lower())
if expected_machine is None:
    raise SystemExit(f"unsupported smoke architecture: {platform.machine()}")
manifest = json.loads(manifest_path.read_text("utf-8"))
if manifest.get("schema") != 1:
    raise SystemExit("unsupported AppImage runtime manifest schema")
records = {entry.get("name"): entry for entry in manifest.get("libraries", [])}
for name in ("libGLESv2.so.2", "libEGL.so.1", "libGLdispatch.so.0"):
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
    record = records.get(name)
    if not isinstance(record, dict):
        raise SystemExit(f"AppImage runtime manifest does not describe: {name}")
    digest = hashlib.sha256(data).hexdigest()
    if record.get("sha256") != digest or record.get("size") != len(data) or record.get("elfMachine") != machine:
        raise SystemExit(f"AppImage runtime manifest integrity mismatch: {name}")
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
