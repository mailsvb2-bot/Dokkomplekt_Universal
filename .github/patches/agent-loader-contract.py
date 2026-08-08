from pathlib import Path

path = Path("src-tauri/src/subsystems/document_commands.rs")
text = path.read_text(encoding="utf-8")
old = '''    let loaded_case = repo.load_case("current").map_err(|error| error.to_string())?;
    let mut loaded_pack = repo.load_pack("default").map_err(|error| error.to_string())?;
    let loaded_license = if load_commercial_state {
'''
new = '''    let loaded_case = repo.load_case("current").map_err(|error| error.to_string())?;
    let loaded_pack = repo.load_pack("default").map_err(|error| error.to_string())?;
    let loaded_license = if load_commercial_state {
'''
if text.count(old) != 1:
    raise SystemExit(f"loaded_pack declaration marker count={text.count(old)}")
text = text.replace(old, new, 1)
old = '''    if let Some(pack) = loaded_pack.as_mut() {
        let rebound = bind_loaded_pack_to_published_template_versions(app, &repo, pack)?;
        if rebound > 0 && load_commercial_state {
            repo.save_pack(pack).map_err(|error| error.to_string())?;
        }
    }

    let mut case_guard = state
'''
new = '''    let loaded_pack = if let Some(mut pack) = loaded_pack {
        let rebound = bind_loaded_pack_to_published_template_versions(app, &repo, &mut pack)?;
        if rebound > 0 && load_commercial_state {
            repo.save_pack(&pack).map_err(|error| error.to_string())?;
        }
        Some(pack)
    } else {
        None
    };

    let mut case_guard = state
'''
if text.count(old) != 1:
    raise SystemExit(f"loaded_pack rebind marker count={text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
