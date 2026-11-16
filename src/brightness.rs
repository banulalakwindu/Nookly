//! Screen brightness: the laptop panel through Windows (WMI), external
//! monitors through their own control channel (DDC/CI), when they allow it.

use windows::core::{BSTR, PCWSTR};
use windows::Win32::Devices::Display::*;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Com::*;
use windows::Win32::System::Variant::*;
use windows::Win32::System::Wmi::*;

/// Brightness 0-100 of the screen the notch is on, if it can be read.
pub fn get(hwnd: HWND) -> Option<u32> {
    ddc_get(hwnd).or_else(wmi_get)
}

/// Set brightness 0-100; returns the value now set.
pub fn set(hwnd: HWND, v: u32) -> Option<u32> {
    let v = v.min(100);
    if ddc_set(hwnd, v) || wmi_set(v) {
        Some(v)
    } else {
        None
    }
}

fn physical(hwnd: HWND) -> Option<Vec<PHYSICAL_MONITOR>> {
    unsafe {
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut n = 0u32;
        GetNumberOfPhysicalMonitorsFromHMONITOR(mon, &mut n).ok()?;
        if n == 0 {
            return None;
        }
        let mut v = vec![PHYSICAL_MONITOR::default(); n as usize];
        GetPhysicalMonitorsFromHMONITOR(mon, &mut v).ok()?;
        Some(v)
    }
}

fn ddc_get(hwnd: HWND) -> Option<u32> {
    let mons = physical(hwnd)?;
    let mut out = None;
    unsafe {
        for m in &mons {
            let (mut lo, mut cur, mut hi) = (0u32, 0u32, 0u32);
            if out.is_none() && GetMonitorBrightness(m.hPhysicalMonitor, &mut lo, &mut cur, &mut hi) != 0 && hi > lo {
                out = Some((cur - lo) * 100 / (hi - lo));
            }
        }
        let _ = DestroyPhysicalMonitors(&mons);
    }
    out
}

fn ddc_set(hwnd: HWND, v: u32) -> bool {
    let Some(mons) = physical(hwnd) else { return false };
    let mut ok = false;
    unsafe {
        for m in &mons {
            let (mut lo, mut cur, mut hi) = (0u32, 0u32, 0u32);
            if GetMonitorBrightness(m.hPhysicalMonitor, &mut lo, &mut cur, &mut hi) != 0 && hi > lo {
                ok |= SetMonitorBrightness(m.hPhysicalMonitor, lo + (hi - lo) * v / 100) != 0;
            }
        }
        let _ = DestroyPhysicalMonitors(&mons);
    }
    ok
}

fn wmi() -> Option<IWbemServices> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let loc: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).ok()?;
        let svc = loc.ConnectServer(&BSTR::from("ROOT\\WMI"), &BSTR::new(), &BSTR::new(), &BSTR::new(), 0, &BSTR::new(), None).ok()?;
        let _ = CoSetProxyBlanket(&svc, 10, 0, PCWSTR::null(), RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, None, EOAC_NONE);
        Some(svc)
    }
}

fn first(svc: &IWbemServices, query: &str) -> Option<IWbemClassObject> {
    unsafe {
        let en = svc.ExecQuery(&BSTR::from("WQL"), &BSTR::from(query), WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY, None).ok()?;
        let mut objs = [None];
        let mut got = 0u32;
        let _ = en.Next(2000, &mut objs, &mut got);
        if got == 0 {
            return None;
        }
        objs[0].take()
    }
}

fn wmi_get() -> Option<u32> {
    let svc = wmi()?;
    let o = first(&svc, "SELECT CurrentBrightness FROM WmiMonitorBrightness")?;
    unsafe {
        let mut v = VARIANT::default();
        o.Get(windows::core::w!("CurrentBrightness"), 0, &mut v, None, None).ok()?;
        let n = VariantToUInt32(&v).ok();
        let _ = VariantClear(&mut v);
        n
    }
}

fn wmi_set(level: u32) -> bool {
    (|| unsafe {
        let svc = wmi()?;
        let inst = first(&svc, "SELECT * FROM WmiMonitorBrightnessMethods")?;
        let mut path = VARIANT::default();
        inst.Get(windows::core::w!("__PATH"), 0, &mut path, None, None).ok()?;
        let path_s = BSTR::try_from(&path).ok()?;
        let _ = VariantClear(&mut path);
        let mut class: Option<IWbemClassObject> = None;
        svc.GetObject(&BSTR::from("WmiMonitorBrightnessMethods"), WBEM_FLAG_RETURN_WBEM_COMPLETE, None, Some(&mut class), None).ok()?;
        let class = class?;
        let mut sig: Option<IWbemClassObject> = None;
        class.GetMethod(windows::core::w!("WmiSetBrightness"), 0, &mut sig, std::ptr::null_mut()).ok()?;
        let args = sig?.SpawnInstance(0).ok()?;
        let timeout = VARIANT::from(1i32);
        let bright = VARIANT::from(level as u8);
        args.Put(windows::core::w!("Timeout"), 0, &timeout, 0).ok()?;
        args.Put(windows::core::w!("Brightness"), 0, &bright, 0).ok()?;
        svc.ExecMethod(&path_s, &BSTR::from("WmiSetBrightness"), WBEM_GENERIC_FLAG_TYPE(0), None, &args, None, None).ok()?;
        Some(())
    })()
    .is_some()
}
