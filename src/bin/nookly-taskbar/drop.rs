//! Files dragged onto the bar: rest on an app to bring it (or the window
//! you rest on in its pop-up) forward and go on into it, as with Windows'
//! taskbar; or let go on it to open them with it.

use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::*;
use windows::core::*;

use crate::{bar_of, on_bar};

#[implement(IDropTarget)]
struct Target {
    hwnd: isize,
}

impl Target_Impl {
    /// What letting go here would do, as far as the source allows it.
    fn effect(&self, pt: &POINTL, over: bool, allowed: DROPEFFECT) -> DROPEFFECT {
        let h = HWND(self.hwnd as *mut _);
        let ok = on_bar(bar_of(h), |b| b.file_drag(Some((pt.x, pt.y)), over)).unwrap_or(false);
        let pick = [DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE].into_iter().find(|e| allowed.0 & e.0 != 0);
        if ok { pick.unwrap_or(DROPEFFECT_NONE) } else { DROPEFFECT_NONE }
    }
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(&self, _d: Ref<IDataObject>, _k: MODIFIERKEYS_FLAGS, pt: &POINTL, eff: *mut DROPEFFECT) -> Result<()> {
        unsafe { *eff = self.effect(pt, true, *eff) };
        Ok(())
    }

    fn DragOver(&self, _k: MODIFIERKEYS_FLAGS, pt: &POINTL, eff: *mut DROPEFFECT) -> Result<()> {
        unsafe { *eff = self.effect(pt, true, *eff) };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        on_bar(bar_of(HWND(self.hwnd as *mut _)), |b| b.file_drag(None, false));
        Ok(())
    }

    fn Drop(&self, d: Ref<IDataObject>, _k: MODIFIERKEYS_FLAGS, pt: &POINTL, eff: *mut DROPEFFECT) -> Result<()> {
        unsafe { *eff = self.effect(pt, false, *eff) };
        let files = d.as_ref().map(files).unwrap_or_default();
        on_bar(bar_of(HWND(self.hwnd as *mut _)), |b| b.file_drop((pt.x, pt.y), &files));
        Ok(())
    }
}

/// Take drops on this window.
pub fn register(hwnd: HWND) {
    let t: IDropTarget = Target { hwnd: hwnd.0 as isize }.into();
    unsafe {
        let _ = RegisterDragDrop(hwnd, &t);
    }
}

/// The files in what's dropped.
fn files(d: &IDataObject) -> Vec<String> {
    let f = FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
    let mut out = Vec::new();
    unsafe {
        if let Ok(mut med) = d.GetData(&f) {
            let h = HDROP(med.u.hGlobal.0);
            for i in 0..DragQueryFileW(h, 0xFFFF_FFFF, None) {
                let len = DragQueryFileW(h, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                DragQueryFileW(h, i, Some(&mut buf));
                out.push(String::from_utf16_lossy(&buf[..len]));
            }
            ReleaseStgMedium(&mut med);
        }
    }
    out
}
