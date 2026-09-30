//! The one piece of WebView policy that the page cannot express.
//!
//! WebView2 ships Microsoft Edge's *general* autofill switched on, and general autofill is not a
//! form feature: it is the browser offering saved names, addresses and phone numbers for any text
//! field it thinks it recognises. There is no attribute, header or CSP directive that turns it
//! off, so focusing the search box of a local-network messenger could surface a "saved info"
//! dropdown with details that have nothing to do with this application. The documented switch is
//! WebView2's own settings object, which is what this module reaches for. (Password and payment
//! autofill are already off in WebView2 by default.)
//!
//! It runs once, from `setup`, where the window declared in `tauri.conf.json` already exists.

/// Turns off the web view's saved-info autofill for the main window.
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

/// Nothing to do away from Windows: WebView2 is the only engine used here that has a saved-info
/// autofill to disable.
#[cfg(not(windows))]
pub fn disable_saved_info<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) {}
