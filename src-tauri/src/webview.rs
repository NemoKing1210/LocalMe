//! WebView2's saved-info autofill has no attribute, header or CSP switch, so it is turned off
//! through the settings object; otherwise a local-network messenger's text fields could offer
//! saved names, addresses and phone numbers. It runs once, from `setup`.

/// Turns off saved-info autofill for the main window's web view.
#[cfg(windows)]
pub fn disable_saved_info<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings4;
    use windows_core::Interface;

    let Some(window) = app.get_webview_window(crate::window::MAIN_WINDOW) else {
        tracing::warn!("the main window is missing; the web view cannot be configured");
        return;
    };

    let configured = window.with_webview(|webview| {
        // SAFETY: the controller, the core and the settings are live WebView2 objects belonging to
        // this window, reached from the thread that owns them. The cast widens to a newer revision
        // of the same settings interface, and every failure is reported rather than unwrapped.
        let result = unsafe {
            webview
                .controller()
                .CoreWebView2()
                .and_then(|core| core.Settings())
                .and_then(|settings| settings.cast::<ICoreWebView2Settings4>())
                .and_then(|settings| settings.SetIsGeneralAutofillEnabled(false))
        };

        match result {
            Ok(()) => tracing::debug!("the web view's saved-info autofill is off"),
            Err(error) => {
                tracing::warn!(%error, "the web view's saved-info autofill could not be turned off");
            }
        }
    });

    if let Err(error) = configured {
        tracing::warn!(%error, "the web view could not be reached for configuration");
    }
}

/// Not needed away from Windows: WebView2 is the only engine with saved-info autofill.
#[cfg(not(windows))]
pub fn disable_saved_info<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) {}
