use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const APPIMAGE_GRAPHICS_LIBRARIES: [&str; 3] = [
    "libGLESv2.so.2",
    "libEGL.so.1",
    "libGLdispatch.so.0",
];

fn discover_linux_libraries() -> Result<BTreeMap<String, PathBuf>, String> {
    let output = Command::new("ldconfig")
        .arg("-p")
        .output()
        .map_err(|error| format!("failed to run ldconfig -p: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ldconfig -p failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let mut found = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((left, right)) = line.split_once("=>") else {
            continue;
        };
        let Some(name) = left.split_whitespace().next() else {
            continue;
        };
        if !APPIMAGE_GRAPHICS_LIBRARIES.contains(&name) {
            continue;
        }
        let candidate = PathBuf::from(right.trim());
        if candidate.is_file() {
            found.entry(name.to_string()).or_insert(candidate);
        }
    }
    Ok(found)
}

fn stage_appimage_graphics_runtime() -> Result<(), String> {
    if env::var_os("CARGO_CFG_TARGET_OS").as_deref() != Some(std::ffi::OsStr::new("linux")) {
        return Ok(());
    }
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR is missing")?,
    );
    let destination = manifest_dir.join("linux-runtime");
    let discovered = discover_linux_libraries()?;
    let missing = APPIMAGE_GRAPHICS_LIBRARIES
        .iter()
        .filter(|name| !discovered.contains_key(**name))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "required AppImage graphics libraries are missing: {}. Install libegl1, libgles2 and libglvnd0.",
            missing.join(", ")
        ));
    }
    if destination.exists() {
        fs::remove_dir_all(&destination)
            .map_err(|error| format!("failed to clear {}: {error}", destination.display()))?;
    }
    fs::create_dir_all(&destination)
        .map_err(|error| format!("failed to create {}: {error}", destination.display()))?;
    for name in APPIMAGE_GRAPHICS_LIBRARIES {
        let source = discovered
            .get(name)
            .ok_or_else(|| format!("library disappeared during staging: {name}"))?;
        let target = destination.join(name);
        fs::copy(source, &target).map_err(|error| {
            format!(
                "failed to stage {} as {}: {error}",
                source.display(),
                target.display()
            )
        })?;
        let header = fs::read(&target)
            .map_err(|error| format!("failed to verify {}: {error}", target.display()))?;
        if !header.starts_with(b"\x7fELF") {
            return Err(format!(
                "staged library is not an ELF binary: {}",
                target.display()
            ));
        }
        println!(
            "cargo:warning=staged AppImage runtime {name} from {}",
            source.display()
        );
    }
    Ok(())
}

fn main() {
    if let Err(error) = stage_appimage_graphics_runtime() {
        panic!("Linux AppImage runtime staging failed: {error}");
    }
    tauri_build::build();
}
