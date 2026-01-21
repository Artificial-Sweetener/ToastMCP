use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct NotifyInput {
    pub title: String,
    pub message: String,
    pub sound: String,
    pub icon: String,
}

pub fn notify(input: NotifyInput) -> Result<()> {
    let icon_path = resolve_icon(&input.icon)?;
    if let Some(sound_path) = find_sound_path(&input.sound) {
        let playback_path = prepare_quiet_wav(&sound_path, 0.7).unwrap_or(sound_path);
        play_sound(&playback_path)?;
        show_toast(&input.title, &input.message, Some(icon_path.as_path()), None)?;
        return Ok(());
    }

    if let Some(audio_src) = system_sound_to_audio_src(&input.sound) {
        show_toast(
            &input.title,
            &input.message,
            Some(icon_path.as_path()),
            Some(audio_src),
        )?;
        return Ok(());
    }

    Err(anyhow::anyhow!("Sound not found: {}", input.sound))
}

fn prepare_quiet_wav(path: &Path, volume: f32) -> Result<PathBuf> {
    if !(0.0..=1.0).contains(&volume) {
        return Ok(path.to_path_buf());
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .context("Failed to resolve exe directory")?;
    let cache_dir = exe_dir.join("cache");
    std::fs::create_dir_all(&cache_dir).context("Failed to create cache directory")?;

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("sound");
    let cache_name = format!("{stem}_vol70.wav");
    let cache_path = cache_dir.join(cache_name);

    if cache_path.exists() {
        let src_time = std::fs::metadata(path)?.modified().ok();
        let dst_time = std::fs::metadata(&cache_path)?.modified().ok();
        if src_time.is_some() && dst_time.is_some() && dst_time >= src_time {
            return Ok(cache_path);
        }
    }

    let mut data = std::fs::read(path).context("Failed to read wav file")?;
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Ok(path.to_path_buf());
    }

    let mut cursor = 12;
    let mut fmt_chunk: Option<(u16, u16, u16)> = None;
    let mut data_chunk: Option<(usize, usize)> = None;

    while cursor + 8 <= data.len() {
        let chunk_id = &data[cursor..cursor + 4];
        let chunk_size = u32::from_le_bytes([
            data[cursor + 4],
            data[cursor + 5],
            data[cursor + 6],
            data[cursor + 7],
        ]) as usize;
        let chunk_start = cursor + 8;
        let chunk_end = chunk_start.saturating_add(chunk_size);
        if chunk_end > data.len() {
            break;
        }

        if chunk_id == b"fmt " && chunk_size >= 16 {
            let audio_format = u16::from_le_bytes([data[chunk_start], data[chunk_start + 1]]);
            let channels = u16::from_le_bytes([data[chunk_start + 2], data[chunk_start + 3]]);
            let bits_per_sample = u16::from_le_bytes([
                data[chunk_start + 14],
                data[chunk_start + 15],
            ]);
            fmt_chunk = Some((audio_format, channels, bits_per_sample));
        } else if chunk_id == b"data" {
            data_chunk = Some((chunk_start, chunk_size));
        }

        cursor = chunk_end + (chunk_size % 2);
    }

    let Some((audio_format, _channels, bits_per_sample)) = fmt_chunk else {
        return Ok(path.to_path_buf());
    };
    let Some((data_start, data_size)) = data_chunk else {
        return Ok(path.to_path_buf());
    };
    if audio_format != 1 || bits_per_sample != 16 {
        return Ok(path.to_path_buf());
    }

    let data_end = data_start.saturating_add(data_size).min(data.len());
    let mut i = data_start;
    while i + 1 < data_end {
        let sample = i16::from_le_bytes([data[i], data[i + 1]]);
        let scaled = (sample as f32 * volume)
            .round()
            .clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        let bytes = scaled.to_le_bytes();
        data[i] = bytes[0];
        data[i + 1] = bytes[1];
        i += 2;
    }

    std::fs::write(&cache_path, &data).context("Failed to write cached wav")?;
    Ok(cache_path)
}

fn resolve_sound(sound_id: &str) -> Result<PathBuf> {
    let file_name = format!("{sound_id}.wav");
    resolve_asset("sounds", &file_name)
}

fn find_sound_path(sound_id: &str) -> Option<PathBuf> {
    resolve_sound(sound_id).ok()
}

fn resolve_icon(icon_id: &str) -> Result<PathBuf> {
    let file_name = format!("{icon_id}.png");
    resolve_asset("icons", &file_name)
}

fn resolve_asset(folder: &str, file_name: &str) -> Result<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));

    if let Some(dir) = exe_dir {
        let candidate = dir.join(folder).join(file_name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir.join(folder).join(file_name);
    if candidate.exists() {
        return Ok(candidate);
    }

    Err(anyhow::anyhow!(
        "Missing asset: {}/{}",
        folder,
        file_name
    ))
}

#[cfg(windows)]
fn play_sound(path: &Path) -> Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::Media::Audio::{
        PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT,
    };

    let wide: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        PlaySoundW(
            windows::core::PCWSTR(wide.as_ptr()),
            None,
            SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
        )
        .ok()
        .context("PlaySoundW failed")?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn play_sound(_path: &Path) -> Result<()> {
    Err(anyhow::anyhow!("Sound playback is only implemented on Windows"))
}

#[cfg(windows)]
fn show_toast(
    title: &str,
    message: &str,
    icon_path: Option<&Path>,
    audio_src: Option<&'static str>,
) -> Result<()> {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    use windows::core::HSTRING;

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .context("CoInitializeEx failed")?;
    }

    let app_id = HSTRING::from("ToastMCP");
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(&app_id)
            .context("SetCurrentProcessExplicitAppUserModelID failed")?;
    }
    ensure_start_menu_shortcut("ToastMCP")?;

    let image_fragment = icon_path
        .and_then(|path| path.to_str())
        .map(|path| format!(r#"<image placement="appLogoOverride" src="file:///{path}"/>"#))
        .unwrap_or_default();

    let audio_fragment = audio_src
        .map(|src| format!(r#"<audio src="{src}"/>"#))
        .unwrap_or_else(|| "<audio silent=\"true\"/>".to_string());

    let toast_xml = format!(
        r#"<toast>
  <visual>
    <binding template="ToastGeneric">
      <text>{}</text>
      <text>{}</text>
      {}
    </binding>
  </visual>
  {}
</toast>"#,
        xml_escape(title),
        xml_escape(message),
        image_fragment,
        audio_fragment
    );

    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(toast_xml))?;
    let toast = ToastNotification::CreateToastNotification(&document)?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&app_id)?;
    notifier.Show(&toast)?;
    Ok(())
}

#[cfg(not(windows))]
fn show_toast(
    _title: &str,
    _message: &str,
    _icon_path: Option<&Path>,
    _audio_src: Option<&'static str>,
) -> Result<()> {
    Err(anyhow::anyhow!("Toast notifications are only implemented on Windows"))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn system_sound_to_audio_src(sound_id: &str) -> Option<&'static str> {
    match sound_id {
        "default" => Some("ms-winsoundevent:Notification.Default"),
        "im" => Some("ms-winsoundevent:Notification.IM"),
        "mail" => Some("ms-winsoundevent:Notification.Mail"),
        "reminder" => Some("ms-winsoundevent:Notification.Reminder"),
        "sms" => Some("ms-winsoundevent:Notification.SMS"),
        "alarm" => Some("ms-winsoundevent:Notification.Alarm"),
        "incoming_call" => Some("ms-winsoundevent:Notification.IncomingCall"),
        _ => None,
    }
}


#[cfg(windows)]
fn ensure_start_menu_shortcut(app_id: &str) -> Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::IShellLinkW;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::core::{Interface, PROPVARIANT};

    let appdata = std::env::var("APPDATA").context("APPDATA not set")?;
    let shortcut_path = std::path::PathBuf::from(appdata)
        .join("Microsoft\\Windows\\Start Menu\\Programs\\ToastMCP.lnk");

    if shortcut_path.exists() {
        let _ = std::fs::remove_file(&shortcut_path);
    }

    let exe_path = std::env::current_exe().context("Failed to resolve current exe")?;
    let icon_path = exe_path
        .parent()
        .map(|dir| dir.join("res\\ToastMCP.ico"))
        .filter(|path| path.exists());

    let link: IShellLinkW = unsafe { CoCreateInstance(&windows::Win32::UI::Shell::ShellLink, None, CLSCTX_INPROC_SERVER)? };
    let exe_wide: Vec<u16> = OsStr::new(&exe_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        link.SetPath(windows::core::PCWSTR(exe_wide.as_ptr()))
            .context("SetPath failed")?;
        if let Some(icon_path) = icon_path.as_ref() {
            let icon_wide: Vec<u16> = OsStr::new(icon_path)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            link.SetIconLocation(windows::core::PCWSTR(icon_wide.as_ptr()), 0)
                .context("SetIconLocation failed")?;
        }
    }

    unsafe {
        let propvariant = PROPVARIANT::from(app_id);
        let store = link.cast::<IPropertyStore>()?;
        store
            .SetValue(&PKEY_AppUserModel_ID, &propvariant)
            .context("SetValue AppUserModelID failed")?;
        store.Commit().context("Commit AppUserModelID failed")?;
    }

    let persist: IPersistFile = link.cast()?;
    let shortcut_wide: Vec<u16> = OsStr::new(&shortcut_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        persist
            .Save(windows::core::PCWSTR(shortcut_wide.as_ptr()), true)
            .context("Save shortcut failed")?;
    }

    Ok(())
}
