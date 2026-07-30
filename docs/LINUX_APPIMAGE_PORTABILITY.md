# Linux AppImage portability

Dokkomplekt Universal treats the AppImage as a self-contained desktop package rather than relying on graphics libraries that happen to be installed on the build runner.

## Bundled graphics runtime

Immediately before the Tauri bundle phase, `scripts/stage_linux_appimage_runtime.mjs` resolves these GLVND libraries through `ldconfig`:

- `libGLESv2.so.2`;
- `libEGL.so.1`;
- `libGLdispatch.so.0`.

The staging step is Linux-only and runs through `build.beforeBundleCommand`. Ordinary `cargo check`, `cargo test` and Tauri development builds do not modify the source tree or require the packaging runtime.

Each library must be a regular, non-empty 64-bit ELF for the target architecture. Missing, stale, non-ELF or wrong-architecture candidates stop packaging. The staged directory is generated under `src-tauri/target/appimage-runtime` and is not committed.

## Integrity evidence

The AppImage includes `usr/share/dokkomplekt/appimage-runtime.json`. It records, for every bundled library:

- file name;
- byte size;
- SHA-256;
- ELF machine identifier.

The installer contract extracts the finished AppImage and independently verifies the files against this manifest. Merely finding a file with the expected name is not sufficient.

## Launch evidence

After structural and integrity checks, the Linux installer contract starts the actual AppImage under an isolated Xvfb and D-Bus session with `APPIMAGE_EXTRACT_AND_RUN=1`. The full process group must remain alive through the smoke interval. An application that flashes and exits is treated as a failed package even when extraction itself succeeded.

DEB validation remains separate and continues to perform install, GUI launch and uninstall checks. RPM contents are inspected independently.

## Packaging prerequisites

The Linux packaging host must provide target-architecture packages containing EGL, GLESv2 and GLVND dispatch libraries. On Debian/Ubuntu runners these are normally supplied by `libegl1`, `libgles2` and `libglvnd0` together with the WebKitGTK build dependencies. Packaging fails closed when they are unavailable.
