use serde::{Deserialize, Serialize};

const REPO: &str = "OmarKrypton/Payment_App";
const ASSET_NAME: &str = "vouchify-linux-x86_64";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeUpdateInfo {
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub error: Option<String>,
}

fn ver_tuple(v: &str) -> (u64, u64, u64) {
    let mut it = v.split('.').map(|p| p.parse().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

fn newer(a: &str, b: &str) -> bool {
    ver_tuple(a) > ver_tuple(b)
}

fn ensure_rustls_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

fn is_elf(b: &[u8]) -> bool {
    b.len() > 4 && b[0] == 0x7f && b[1] == b'E' && b[2] == b'L' && b[3] == b'F'
}

#[tauri::command]
pub async fn check_native_update() -> NativeUpdateInfo {
    ensure_rustls_provider();

    let current = env!("CARGO_PKG_VERSION").to_string();
    let mut info = NativeUpdateInfo {
        available: false,
        current: current.clone(),
        latest: current.clone(),
        error: None,
    };

    if std::env::var("APPIMAGE").is_ok() {
        return info;
    }

    let client = match reqwest::Client::builder()
        .user_agent("VouchifyUpdater")
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            info.error = Some(e.to_string());
            return info;
        }
    };

    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            info.error = Some(e.to_string());
            return info;
        }
    };

    if !resp.status().is_success() {
        info.error = Some(format!("GitHub API status {}", resp.status()));
        return info;
    }

    let json: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            info.error = Some(e.to_string());
            return info;
        }
    };

    let tag = json["tag_name"].as_str().unwrap_or("").to_string();
    let latest = tag.trim_start_matches('v').to_string();
    let has_asset = json["assets"]
        .as_array()
        .map(|a| a.iter().any(|x| x["name"].as_str() == Some(ASSET_NAME)))
        .unwrap_or(false);

    if !latest.is_empty() {
        info.latest = latest.clone();
        info.available = has_asset && newer(&latest, &current);
    }
    info
}

#[tauri::command]
pub async fn install_native_update(version: String) -> Result<(), String> {
    ensure_rustls_provider();

    #[cfg(not(target_os = "linux"))]
    {
        let _ = version;
        return Err("native updater is only supported on Linux".into());
    }

    #[cfg(target_os = "linux")]
    {
        let client = reqwest::Client::builder()
            .user_agent("VouchifyUpdater")
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| e.to_string())?;

        let url = format!(
            "https://github.com/{REPO}/releases/download/v{}/{}",
            version, ASSET_NAME
        );
        let bytes = client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .bytes()
            .await
            .map_err(|e| e.to_string())?;

        if !is_elf(&bytes) {
            return Err("downloaded file is not an executable".into());
        }
        if bytes.len() < 5_000_000 {
            return Err("downloaded file looks too small".into());
        }

        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let dir = exe
            .parent()
            .ok_or_else(|| "no parent directory for executable".to_string())?
            .to_path_buf();
        let tmp = dir.join(format!(".vouchify.new.{}", std::process::id()));
        std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;

        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;

        let pid = std::process::id();
        let script = dir.join(format!(".vouchify-swap.{}", pid));
        let swap = format!(
            "#!/bin/sh\nfor i in 1 2 3 4 5 6 7 8 9 10; do\n  kill -0 {} 2>/dev/null || break\n  sleep 1\ndone\nmv -f '{}' '{}'\nchmod +x '{}'\nexec '{}'\n",
            pid,
            tmp.display(),
            exe.display(),
            exe.display(),
            exe.display()
        );
        std::fs::write(&script, swap).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;

        std::process::Command::new("setsid")
            .arg(&script)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}