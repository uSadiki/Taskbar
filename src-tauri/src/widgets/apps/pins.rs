//! The apps pinned to the Windows taskbar.
//!
//! Windows keeps the pin list, in order, in the binary `Favorites` value under
//! `Explorer\Taskband`. Each entry is a shell item id list: a shortcut in the
//! pin folder for desktop apps, an Apps-folder item for Store apps. The value
//! is only ever read. If it can't be parsed, the `.lnk` files in the pin
//! folder are used instead (no Store apps, ordered by name).
//!
//! Must be called on the shell worker thread.

use std::path::PathBuf;

use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, IPersistFile, STGM_READ,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RegGetValueW};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{
    IShellItem2, IShellLinkW, SHCreateItemFromIDList, SHCreateItemFromParsingName,
    SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY, ShellLink,
};
use windows::core::{HSTRING, Interface, PWSTR, w};

use super::model::PinMeta;

/// How to start a pinned app and where its icon comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellRef {
    /// An absolute item id list from the pin list.
    IdList(Vec<u8>),
    /// A file path or shell parsing name.
    Path(String),
}

#[derive(Debug, Clone)]
pub struct Pin {
    pub meta: PinMeta,
    pub shell: ShellRef,
}

/// Split the `Favorites` blob into item id lists. The layout is a sequence of
/// `[0x00][u32 length][length bytes]` records ended by `0xFF`.
pub fn parse_favorites(blob: &[u8]) -> Vec<Vec<u8>> {
    let mut entries = Vec::new();
    let mut pos = 0;
    while pos + 5 <= blob.len() && blob[pos] != 0xFF {
        let len = u32::from_le_bytes([blob[pos + 1], blob[pos + 2], blob[pos + 3], blob[pos + 4]])
            as usize;
        let start = pos + 5;
        let Some(end) = start.checked_add(len).filter(|end| *end <= blob.len()) else {
            break;
        };
        // An id list is at least its two-byte terminator.
        if len >= 2 {
            entries.push(blob[start..end].to_vec());
        }
        pos = end;
    }
    entries
}

fn read_favorites() -> Option<Vec<u8>> {
    let key = w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Taskband");
    let value = w!("Favorites");
    unsafe {
        let mut size = 0u32;
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            value,
            RRF_RT_REG_BINARY,
            None,
            None,
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let mut data = vec![0u8; size as usize];
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            value,
            RRF_RT_REG_BINARY,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
        data.truncate(size as usize);
        Some(data)
    }
}

unsafe fn take(value: PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }
    let text = unsafe { value.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(value.0 as *const _)) };
    text
}

fn non_empty(text: String) -> Option<String> {
    (!text.trim().is_empty()).then_some(text)
}

/// Target executable and AppUserModelID of a shortcut file.
fn read_shortcut(path: &str) -> (Option<String>, Option<String>) {
    unsafe {
        let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER)
        else {
            return (None, None);
        };
        let Ok(file) = link.cast::<IPersistFile>() else {
            return (None, None);
        };
        if file.Load(&HSTRING::from(path), STGM_READ).is_err() {
            return (None, None);
        }
        let mut buffer = [0u16; 1024];
        let target = link
            .GetPath(&mut buffer, std::ptr::null_mut(), 0)
            .ok()
            .map(|()| {
                let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
                String::from_utf16_lossy(&buffer[..len])
            })
            .and_then(non_empty);
        let aumid = link.cast::<IPropertyStore>().ok().and_then(|store| {
            let mut value = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
            let text = PropVariantToStringAlloc(&value).ok().map(|p| take(p));
            let _ = PropVariantClear(&mut value);
            text.and_then(non_empty)
        });
        (target, aumid)
    }
}

/// Build a pin from a shell item.
fn describe(item: &IShellItem2, shell: ShellRef) -> Option<Pin> {
    unsafe {
        let name = take(item.GetDisplayName(SIGDN_NORMALDISPLAY).ok()?);
        let parsing = take(item.GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING).ok()?);
        let mut aumid = item
            .GetString(&PKEY_AppUserModel_ID)
            .ok()
            .map(|p| take(p))
            .and_then(non_empty);
        let mut target = None;

        if parsing.to_ascii_lowercase().ends_with(".lnk") {
            let (link_target, link_aumid) = read_shortcut(&parsing);
            target = link_target;
            aumid = aumid.or(link_aumid);
        } else if aumid.is_none() {
            // An Apps-folder item's parsing name ends with its AppUserModelID.
            aumid = parsing
                .rsplit('\\')
                .next()
                .filter(|last| last.contains('!'))
                .map(str::to_string);
        }

        let key = aumid
            .clone()
            .or_else(|| target.clone())
            .unwrap_or_else(|| parsing.clone());
        Some(Pin {
            meta: PinMeta {
                key,
                name,
                aumid,
                target,
            },
            shell,
        })
    }
}

fn from_registry() -> Vec<Pin> {
    let Some(blob) = read_favorites() else {
        return Vec::new();
    };
    parse_favorites(&blob)
        .into_iter()
        .filter_map(|id_list| {
            let item: IShellItem2 =
                unsafe { SHCreateItemFromIDList(id_list.as_ptr() as *const ITEMIDLIST) }.ok()?;
            describe(&item, ShellRef::IdList(id_list))
        })
        .collect()
}

pub fn pin_folder() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("APPDATA")?)
        .join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar");
    path.is_dir().then_some(path)
}

fn from_folder() -> Vec<Pin> {
    let Some(dir) = pin_folder() else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|path| {
            let path = path.to_string_lossy().into_owned();
            let item: IShellItem2 =
                unsafe { SHCreateItemFromParsingName(&HSTRING::from(path.as_str()), None) }.ok()?;
            describe(&item, ShellRef::Path(path))
        })
        .collect()
}

/// The pinned apps, in taskbar order.
pub fn read() -> Vec<Pin> {
    let pins = from_registry();
    if !pins.is_empty() {
        return pins;
    }
    let pins = from_folder();
    log::info!(
        "apps: the taskbar pin list was unreadable or empty; using {} shortcut(s) from the pin folder",
        pins.len()
    );
    pins
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0x00];
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn splits_records_until_the_terminator() {
        let mut blob = record(&[1, 2, 3, 0, 0]);
        blob.extend(record(&[9, 9, 0, 0]));
        blob.push(0xFF);
        assert_eq!(
            parse_favorites(&blob),
            vec![vec![1, 2, 3, 0, 0], vec![9, 9, 0, 0]]
        );
    }

    #[test]
    fn empty_and_terminator_only_blobs_have_no_entries() {
        assert!(parse_favorites(&[]).is_empty());
        assert!(parse_favorites(&[0xFF]).is_empty());
    }

    #[test]
    fn stops_at_a_record_that_runs_past_the_end() {
        let mut blob = record(&[1, 2, 0, 0]);
        // Claims 100 bytes but only 3 follow.
        blob.extend_from_slice(&[0x00, 100, 0, 0, 0, 7, 7, 7]);
        assert_eq!(parse_favorites(&blob), vec![vec![1, 2, 0, 0]]);
    }

    #[test]
    fn a_huge_length_cannot_overflow() {
        let blob = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 1, 2, 3];
        assert!(parse_favorites(&blob).is_empty());
    }

    #[test]
    fn skips_records_too_short_to_be_an_id_list() {
        let mut blob = record(&[0]);
        blob.extend(record(&[5, 5, 0, 0]));
        blob.push(0xFF);
        assert_eq!(parse_favorites(&blob), vec![vec![5, 5, 0, 0]]);
    }
}
