//! Share files with Windows' own Share window (Nearby Share, Phone Link,
//! mail, chat apps): drop them on the Share side of the notch.

use std::sync::Mutex;

use windows::core::*;
use windows::ApplicationModel::DataTransfer::*;
use windows::Foundation::Collections::IIterable;
use windows::Foundation::{EventRegistrationToken, TypedEventHandler};
use windows::Storage::{IStorageItem, StorageFile, StorageFolder};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::IDataTransferManagerInterop;

/// What the next share hands over, and the handler registered for it.
static PENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());
static TOKEN: Mutex<Option<(isize, i64)>> = Mutex::new(None);

fn item(path: &str) -> Option<IStorageItem> {
    let p = HSTRING::from(path);
    if std::path::Path::new(path).is_dir() {
        StorageFolder::GetFolderFromPathAsync(&p).ok()?.get().ok()?.cast().ok()
    } else {
        StorageFile::GetFileFromPathAsync(&p).ok()?.get().ok()?.cast().ok()
    }
}

/// Open the Share window for `paths`, anchored to the notch window.
pub fn share(hwnd: HWND, paths: Vec<String>) -> bool {
    if paths.is_empty() {
        return false;
    }
    *PENDING.lock().unwrap() = paths;
    (|| -> Result<()> {
        let interop: IDataTransferManagerInterop = factory::<DataTransferManager, IDataTransferManagerInterop>()?;
        let dtm: DataTransferManager = unsafe { interop.GetForWindow(hwnd)? };
        // One handler per window, registered once.
        let mut tok = TOKEN.lock().unwrap();
        if tok.map_or(true, |(h, _)| h != hwnd.0 as isize) {
            if let Some((_, t)) = tok.take() {
                let _ = dtm.RemoveDataRequested(EventRegistrationToken { Value: t });
            }
            let t = dtm.DataRequested(&TypedEventHandler::new(|_m: &Option<DataTransferManager>, args: &Option<DataRequestedEventArgs>| {
                let Some(args) = args else { return Ok(()) };
                let req = args.Request()?;
                let data = req.Data()?;
                let paths = PENDING.lock().unwrap().clone();
                let items: Vec<Option<IStorageItem>> = paths.iter().map(|p| item(p)).filter(|i| i.is_some()).collect();
                if items.is_empty() {
                    return req.FailWithDisplayText(&HSTRING::from("Nookly couldn't open those files"));
                }
                let names: Vec<String> = paths.iter().filter_map(|p| std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned())).collect();
                let title = if names.len() == 1 { names[0].clone() } else { format!("{} files", names.len()) };
                data.Properties()?.SetTitle(&HSTRING::from(title))?;
                let list = IIterable::<IStorageItem>::try_from(items)?;
                data.SetStorageItemsReadOnly(&list)?;
                Ok(())
            }))?;
            *tok = Some((hwnd.0 as isize, t.Value));
        }
        drop(tok);
        unsafe { interop.ShowShareUIForWindow(hwnd)? };
        Ok(())
    })()
    .is_ok()
}
