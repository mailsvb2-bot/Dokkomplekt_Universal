use crate::semantic_model::LocalSemanticModelConfig;
use crate::universal_intake;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(crate) struct ManagedSemanticRuntime {
    child: Child,
    endpoint: String,
    model_name: String,
    server_path: PathBuf,
    model_path: PathBuf,
}

impl ManagedSemanticRuntime {
    fn is_healthy_for(&mut self, server_path: &Path, model_path: &Path) -> bool {
        self.server_path == server_path
            && self.model_path == model_path
            && self.child.try_wait().ok().flatten().is_none()
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ManagedSemanticRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Turns a verified downloaded llama.cpp + GGUF component into a live localhost
/// service. An external localhost server remains supported when the component is
/// absent; downloaded files are never reported as usable without a healthy child.
pub(crate) fn effective_config(
    slot: &Mutex<Option<ManagedSemanticRuntime>>,
    configured: &LocalSemanticModelConfig,
) -> Result<LocalSemanticModelConfig, String> {
    if !configured.enabled || configured.provider.trim() != "llama_cpp" {
        return Ok(configured.clone());
    }
    let server_path = universal_intake::resolve_tool("llama_cpp");
    let model_path = universal_intake::resolve_tool("semantic_model");
    if !server_path.is_file() || !model_path.is_file() {
        return Ok(configured.clone());
    }

    let mut guard = slot
        .lock()
        .map_err(|_| "semantic runtime lock poisoned".to_string())?;
    if let Some(runtime) = guard.as_mut() {
        if runtime.is_healthy_for(&server_path, &model_path) {
            return Ok(overridden_config(configured, runtime));
        }
        runtime.stop();
        *guard = None;
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("Не удалось зарезервировать порт llama.cpp: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Не удалось определить порт llama.cpp: {error}"))?
        .port();
    drop(listener);

    let mut command = Command::new(&server_path);
    command
        .arg("--model")
        .arg(&model_path)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--ctx-size")
        .arg("8192")
        .arg("--n-gpu-layers")
        .arg("0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_no_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Не удалось запустить проверенный llama.cpp runtime: {error}"))?;
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok() {
            break;
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Не удалось проверить llama.cpp runtime: {error}"))?
        {
            return Err(format!(
                "llama.cpp runtime завершился до готовности: {status}"
            ));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("llama.cpp runtime не стал готов за 45 секунд".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    let model_name = model_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("dokkomplekt-local")
        .to_string();
    let runtime = ManagedSemanticRuntime {
        child,
        endpoint: format!("http://127.0.0.1:{port}"),
        model_name,
        server_path,
        model_path,
    };
    let effective = overridden_config(configured, &runtime);
    *guard = Some(runtime);
    Ok(effective)
}

fn overridden_config(
    configured: &LocalSemanticModelConfig,
    runtime: &ManagedSemanticRuntime,
) -> LocalSemanticModelConfig {
    let mut effective = configured.clone();
    effective.endpoint = runtime.endpoint.clone();
    effective.model = runtime.model_name.clone();
    effective
}

#[cfg(target_os = "windows")]
fn apply_no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn apply_no_window(_command: &mut Command) {}
