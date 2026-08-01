#!/usr/bin/env bash
set -euo pipefail

# GitHub's Xvfb runner has no usable DRI3/GBM device. Keep production defaults
# untouched, but force this installer-smoke entrypoint onto WebKitGTK's X11
# software path so the gate must observe real pixels instead of a live blank window.
export GDK_BACKEND=x11
export LIBGL_ALWAYS_SOFTWARE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
exec bash "$script_dir/linux_installer_contract_core.sh" "$@"
