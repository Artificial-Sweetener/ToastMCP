use std::path::PathBuf;

pub fn list_icon_ids() -> Vec<String> {
    let mut ids = Vec::new();
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("icons"));
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons"));

    for dir in candidates {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("png") {
                    continue;
                }
                if path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|name| name.to_str())
                    .map(|name| name.eq_ignore_ascii_case("backup"))
                    .unwrap_or(false)
                {
                    continue;
                }
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    ids.push(stem.to_string());
                }
            }
        }
    }

    ids.sort();
    ids.dedup();
    ids
}

pub fn list_sound_ids() -> Vec<String> {
    const WINDOWS_SOUND_IDS: &[&str] = &[
        "default",
        "im",
        "mail",
        "reminder",
        "sms",
        "alarm",
        "incoming_call",
    ];

    let mut ids = Vec::new();
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("sounds"));
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sounds"));

    for dir in candidates {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("wav") {
                    continue;
                }
                if path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|name| name.to_str())
                    .map(|name| name.eq_ignore_ascii_case("backup"))
                    .unwrap_or(false)
                {
                    continue;
                }
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    ids.push(stem.to_string());
                }
            }
        }
    }

    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return WINDOWS_SOUND_IDS.iter().map(|s| s.to_string()).collect();
    }
    ids
}
