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

if requires appimage; then
  [ -n "$appimage" ] || { echo "AppImage not found" >&2; exit 1; }
fi
if requires deb; then
  [ -n "$deb" ] || { echo "deb package not found" >&2; exit 1; }
fi

run_deb_install_smoke() {
  [ "${DOKKOMPLEKT_SKIP_LINUX_INSTALL_SMOKE:-0}" != "1" ] || return 0

  command -v dpkg >/dev/null || { echo "dpkg is required for install smoke" >&2; exit 1; }
  command -v xvfb-run >/dev/null || { echo "xvfb-run is required for launch smoke" >&2; exit 1; }
  command -v dbus-run-session >/dev/null || { echo "dbus-run-session is required for launch smoke" >&2; exit 1; }
  command -v setsid >/dev/null || { echo "setsid is required for isolated launch smoke" >&2; exit 1; }

  local package_name install_log remove_log binary_path smoke_home pid status package_files
  package_name="$(dpkg-deb -f "$deb" Package)"
  install_log="$(mktemp)"
  remove_log="$(mktemp)"
  smoke_home="$(mktemp -d)"
  cleanup_paths+=("$install_log" "$remove_log" "$smoke_home")

  local -a privilege=()
  if [ "$(id -u)" -ne 0 ]; then
    command -v sudo >/dev/null || { echo "sudo is required for deb install smoke" >&2; exit 1; }
    privilege=(sudo)
  fi

  cleanup_deb_install() {
    if dpkg-query -W -f='${Status}' "$package_name" 2>/dev/null | grep -q 'install ok installed'; then
      "${privilege[@]}" dpkg --remove "$package_name" >"$remove_log" 2>&1 || true
    fi
  }

  cleanup_deb_install
  "${privilege[@]}" dpkg --install "$deb" >"$install_log" 2>&1 || {
    cat "$install_log" >&2
    echo "deb install smoke failed" >&2
    exit 1
  }

  package_files="$(dpkg-query -L "$package_name")"
  binary_path="$(awk '/\/(usr\/)?bin\/[^/]*dokkomplekt/ { print; exit }' <<<"$package_files")"
  [ -n "$binary_path" ] && [ -x "$binary_path" ] || {
    echo "installed executable was not found for $package_name" >&2
    cleanup_deb_install
    exit 1
  }

  HOME="$smoke_home" XDG_CONFIG_HOME="$smoke_home/config" XDG_DATA_HOME="$smoke_home/data" \
    setsid xvfb-run -a dbus-run-session -- "$binary_path" >"$smoke_home/launch.log" 2>&1 &
  pid=$!
  sleep 5
  if kill -0 "$pid" 2>/dev/null; then
    status=0
  else
    wait "$pid" || status=$?
    status="${status:-0}"
    if [ "$status" -ne 0 ]; then
      cat "$smoke_home/launch.log" >&2
      cleanup_deb_install
      echo "installed application exited during launch smoke with code $status" >&2
      exit 1
    fi
  fi

  # Stop the complete isolated launch session, not only the xvfb-run wrapper.
  # GTK/Mesa helpers may otherwise survive briefly and race with temp cleanup.
  kill -TERM -- "-$pid" 2>/dev/null || true
  for _ in 1 2 3 4 5; do
    kill -0 -- "-$pid" 2>/dev/null || break
    sleep 0.2
  done
  kill -KILL -- "-$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true

  cleanup_deb_install
  if dpkg-query -W -f='${Status}' "$package_name" 2>/dev/null | grep -q 'install ok installed'; then
    cat "$remove_log" >&2
    echo "deb uninstall smoke did not remove $package_name" >&2
    exit 1
  fi

  printf -- '- deb install/launch/uninstall smoke: OK (%s)\n' "$package_name"
}
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

cleanup_paths=()
cleanup() {
  local path attempt
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
  executable_payload="$(find "$extract_dir/squashfs-root" -type f -perm -u+x \
    \( -name 'AppRun' -o -iname '*dokkomplekt*' \) -print -quit)"
  [ -n "$executable_payload" ] || {
    echo "AppImage does not contain an executable application payload" >&2
    exit 1
  }
fi

if requires deb; then
  run_deb_install_smoke
fi

printf 'Linux bundle validation OK (required: %s)\n' "$required_bundles"
[ -z "$appimage" ] || printf -- '- AppImage: %s\n' "$appimage"
[ -z "$deb" ] || printf -- '- deb: %s\n' "$deb"
[ -z "$rpm_package" ] || printf -- '- rpm: %s\n' "$rpm_package"
