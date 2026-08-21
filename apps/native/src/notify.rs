#[cfg(any(windows, test))]
pub const APP_ID: &str = "xpepelok.cutix";

#[cfg(windows)]
mod windows_impl {
    use super::APP_ID;
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::core::{Interface, HSTRING, PCWSTR};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::StructuredStorage::{
        InitPropVariantFromStringVector, PROPVARIANT,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{
        IShellLinkW, SetCurrentProcessExplicitAppUserModelID, ShellLink,
    };
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    static ALIVE: std::sync::Mutex<Vec<ToastNotification>> = std::sync::Mutex::new(Vec::new());

    static PREPARED: AtomicBool = AtomicBool::new(false);

    const PKEY_APP_ID: PROPERTYKEY = PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3),
        pid: 5,
    };

    fn shortcut_path() -> Option<std::path::PathBuf> {
        let base = dirs::data_dir()?;
        Some(
            base.join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("cutix.lnk"),
        )
    }

    fn write_shortcut() -> windows::core::Result<()> {
        let Some(path) = shortcut_path() else {
            return Ok(());
        };
        if path.is_file() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let Ok(exe) = std::env::current_exe() else {
            return Ok(());
        };

        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(&HSTRING::from(exe.as_os_str()))?;
            if let Some(folder) = exe.parent() {
                link.SetWorkingDirectory(&HSTRING::from(folder.as_os_str()))?;
            }

            let store: IPropertyStore = link.cast()?;
            let identifier = HSTRING::from(APP_ID);
            let value: PROPVARIANT =
                InitPropVariantFromStringVector(Some(&[PCWSTR(identifier.as_ptr())]))?;
            store.SetValue(&PKEY_APP_ID, &value)?;
            store.Commit()?;

            let file: IPersistFile = link.cast()?;
            file.Save(&HSTRING::from(path.as_os_str()), true)?;
        }
        Ok(())
    }

    pub fn prepare() {
        if PREPARED.swap(true, Ordering::SeqCst) {
            return;
        }
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_ID));
        }
        let _ = write_shortcut();
    }

    fn escape(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    pub fn copy_to_clipboard(text: &str) -> bool {
        use windows::Win32::Foundation::{HANDLE, HGLOBAL};
        use windows::Win32::System::DataExchange::{
            CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
        };
        use windows::Win32::System::Memory::{
            GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
        };

        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            if OpenClipboard(None).is_err() {
                return false;
            }
            let _ = EmptyClipboard();
            let Ok(block): windows::core::Result<HGLOBAL> =
                GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)
            else {
                let _ = CloseClipboard();
                return false;
            };
            let target = GlobalLock(block) as *mut u16;
            if target.is_null() {
                let _ = CloseClipboard();
                return false;
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
            let _ = GlobalUnlock(block);
            let placed = SetClipboardData(13, Some(HANDLE(block.0))).is_ok();
            let _ = CloseClipboard();
            placed
        }
    }

    pub fn show(title: &str, body: &str, url: &str) -> bool {
        prepare();

        let payload = format!(
            "<toast activationType=\"protocol\" launch=\"{}\">\
             <visual><binding template=\"ToastGeneric\">\
             <text>{}</text><text>{}</text>\
             </binding></visual>\
             <audio src=\"ms-winsoundevent:Notification.Default\"/>\
             </toast>",
            escape(url),
            escape(title),
            escape(body)
        );

        let Ok(document) = XmlDocument::new() else {
            return false;
        };
        if document.LoadXml(&HSTRING::from(payload)).is_err() {
            return false;
        }

        let Ok(toast) = ToastNotification::CreateToastNotification(&document) else {
            return false;
        };
        let link = url.to_string();
        let _ = toast.Activated(&TypedEventHandler::new(
            move |_: windows::core::Ref<'_, ToastNotification>,
                  _: windows::core::Ref<'_, windows::core::IInspectable>| {
                copy_to_clipboard(&link);
                Ok(())
            },
        ));

        let Ok(notifier) =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))
        else {
            return false;
        };
        let shown = notifier.Show(&toast).is_ok();
        if shown {
            if let Ok(mut alive) = ALIVE.lock() {
                alive.push(toast);
                if alive.len() > 8 {
                    alive.remove(0);
                }
            }
        }
        shown
    }
}

#[cfg(windows)]
pub fn published(title: &str, url: &str) -> bool {
    let heading = cutix_i18n::t_args("youtube.notify.body", &[("title", title)]);
    let body = cutix_i18n::t("youtube.notify.copy");
    windows_impl::show(&heading, &body, url)
}

#[cfg(not(windows))]
pub fn published(_title: &str, _url: &str) -> bool {
    false
}

#[cfg(windows)]
pub fn centre_own_window(_width: f32, _height: f32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SetWindowPos, SystemParametersInfoW, HWND_TOP, SPI_GETWORKAREA,
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };

    let Some(window) = own_window() else {
        return;
    };

    unsafe {
        let mut work = RECT::default();
        if SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work as *mut RECT as *mut std::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_err()
        {
            return;
        }

        let mut frame = RECT::default();
        if GetWindowRect(window, &mut frame).is_err() {
            return;
        }

        let wide = frame.right - frame.left;
        let high = frame.bottom - frame.top;
        let x = work.left + ((work.right - work.left) - wide) / 2;
        let y = work.top + ((work.bottom - work.top) - high) / 2;

        let _ = SetWindowPos(
            window,
            Some(HWND_TOP),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

#[cfg(windows)]
pub fn desktop_scale() -> f32 {
    use windows::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, ReleaseDC, LOGPIXELSX};
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return 1.0;
        }
        let dpi = GetDeviceCaps(Some(screen), LOGPIXELSX);
        ReleaseDC(None, screen);
        if dpi <= 0 {
            1.0
        } else {
            dpi as f32 / 96.0
        }
    }
}

#[cfg(not(windows))]
pub fn centre_own_window(_width: f32, _height: f32) {}

#[cfg(not(windows))]
pub fn desktop_scale() -> f32 {
    1.0
}

#[cfg(windows)]
pub fn copy_link(url: &str) -> bool {
    windows_impl::copy_to_clipboard(url)
}

#[cfg(not(windows))]
pub fn copy_link(_url: &str) -> bool {
    false
}

#[cfg(any(windows, test))]
pub const MOVE_WITH_MOUSE: u32 = 0xf010 | 0x0002;

#[cfg(windows)]
fn post_system_command(command: u32) -> bool {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_SYSCOMMAND};

    let Some(window) = own_window() else {
        return false;
    };
    unsafe {
        PostMessageW(
            Some(window),
            WM_SYSCOMMAND,
            WPARAM(command as usize),
            LPARAM(0),
        )
    }
    .is_ok()
}

#[cfg(windows)]
pub fn window_is_maximized() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::IsZoomed;

    own_window().is_some_and(|window| unsafe { IsZoomed(window) }.as_bool())
}

#[cfg(windows)]
pub fn minimize_own_window() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::SC_MINIMIZE;

    post_system_command(SC_MINIMIZE)
}

#[cfg(not(windows))]
pub fn minimize_own_window() -> bool {
    false
}

#[cfg(windows)]
pub fn toggle_window_maximized() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{SC_MAXIMIZE, SC_RESTORE};

    post_system_command(if window_is_maximized() {
        SC_RESTORE
    } else {
        SC_MAXIMIZE
    })
}

#[cfg(not(windows))]
pub fn toggle_window_maximized() -> bool {
    false
}

#[cfg(windows)]
pub fn begin_window_drag() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows::Win32::UI::WindowsAndMessaging::SC_RESTORE;

    if own_window().is_none() {
        return false;
    }
    unsafe {
        let _ = ReleaseCapture();
    }
    if window_is_maximized() && !post_system_command(SC_RESTORE) {
        return false;
    }
    post_system_command(MOVE_WITH_MOUSE)
}

#[cfg(windows)]
fn own_window() -> Option<windows::Win32::Foundation::HWND> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };

    unsafe extern "system" fn visit(window: HWND, carry: LPARAM) -> BOOL {
        unsafe {
            let mut owner = 0u32;
            GetWindowThreadProcessId(window, Some(&mut owner));
            if owner == std::process::id() && IsWindowVisible(window).as_bool() {
                let slot = carry.0 as *mut isize;
                if *slot == 0 {
                    *slot = window.0 as isize;
                }
            }
            BOOL(1)
        }
    }

    unsafe {
        let mut found: isize = 0;
        let _ = EnumWindows(Some(visit), LPARAM(&mut found as *mut isize as isize));
        (found != 0).then_some(HWND(found as *mut std::ffi::c_void))
    }
}

#[cfg(not(windows))]
pub fn begin_window_drag() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_heading_names_the_video_and_the_body_says_what_a_click_does() {
        let heading = cutix_i18n::t_args("youtube.notify.body", &[("title", "holiday cut")]);
        assert!(heading.contains("holiday cut"));
        assert!(!heading.contains("{title}"));

        let body = cutix_i18n::t("youtube.notify.copy");
        assert!(!body.is_empty());
        assert_ne!(body, "youtube.notify.copy");
    }

    #[test]
    fn the_identifier_is_the_one_the_shortcut_carries() {
        assert_eq!(super::APP_ID, "xpepelok.cutix");
    }

    #[cfg(windows)]
    #[test]
    fn a_drag_asks_the_system_to_move_the_window_with_the_mouse() {
        use windows::Win32::UI::WindowsAndMessaging::{HTCAPTION, SC_MOVE};

        assert_eq!(super::MOVE_WITH_MOUSE, SC_MOVE | HTCAPTION);
    }

    #[cfg(not(windows))]
    #[test]
    fn without_windows_there_is_nothing_to_move() {
        assert!(!super::begin_window_drag());
        assert!(!super::toggle_window_maximized());
        assert!(!super::minimize_own_window());
    }
}
