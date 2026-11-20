// Many helpers (gfx shapes, palette) exist for future use.
#![allow(dead_code)]
//! Code shared by the notch (`nookly.exe`, src/main.rs) and the optional
//! taskbar add-on (`NooklyTaskbar.exe`, src/bin/nookly-taskbar.rs). Each exe
//! links only what it uses, so the notch download doesn't grow.
//!
//!   barcfg.rs the taskbar's settings (items, order, flags)
//!   feed.rs   widget values shared from the notch to the taskbar
//!   gfx.rs    Direct2D drawing, themes, the notch silhouette
//!   glass.rs  Liquid Glass backdrop capture and lens
//!   ui.rs     immediate-mode widgets and hit testing
//!   store.rs  %APPDATA%\Nookly\*.txt persistence and settings
//!   shell.rs  hide/restore the Windows taskbar, open its flyouts
//!   surfaces.rs  the taskbar widgets' pictures, drawn by the notch
//!   time.rs   local time helpers

pub mod barcfg;
pub mod feed;
pub mod gfx;
pub mod glass;
pub mod shell;
pub mod store;
pub mod surfaces;
pub mod time;
pub mod ui;
