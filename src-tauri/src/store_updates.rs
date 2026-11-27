//! MSIX update availability and the handoff to Microsoft Store.

#[cfg(all(windows, feature = "store"))]
const STORE_LISTING: &str = "ms-windows-store://pdp/?ProductId=9NNK2X23VLWT";

#[tauri::command]
pub async fn check_store_update(_app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(all(windows, feature = "store"))]
    {
        return check_native(&_app).await.map_err(|error| {
            tracing::debug!(%error, "Store update check failed");
            "Cannot check Microsoft Store for updates right now. Try again.".into()
        });
    }
    #[cfg(not(all(windows, feature = "store")))]
    Err("This installation is not managed by Microsoft Store.".into())
}

#[cfg(all(windows, feature = "store"))]
async fn check_native(app: &tauri::AppHandle) -> Result<bool, String> {
    use std::time::Duration;
    use tauri::Manager;
    use windows::{
        core::Interface, ApplicationModel::Package, Services::Store::StoreContext,
        Win32::UI::Shell::IInitializeWithWindow,
    };

    // Initiate StoreContext on the desktop UI thread and associate the main
    // window before invoking Store WinRT APIs. Never block the UI awaiting I/O.
    let ui_app = app.clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        if send.is_closed() {
            return;
        }
        let start = || -> Result<_, String> {
            let window = ui_app
                .get_webview_window("main")
                .ok_or("Main window is unavailable")?;
            let hwnd = window.hwnd().map_err(|error| error.to_string())?;
            let start_for_window = || -> windows::core::Result<_> {
                let family = Package::Current()?.Id()?.FamilyName()?;
                let context = StoreContext::GetDefault()?;
                let owner: IInitializeWithWindow = context.cast()?;
                // hwnd belongs to a live Tauri window on this UI thread.
                unsafe { owner.Initialize(hwnd)? };
                Ok((family, context.GetAppAndOptionalStorePackageUpdatesAsync()?))
            };
            start_for_window().map_err(|error| error.to_string())
        };
        let _ = send.send(start());
    })
    .map_err(|error| error.to_string())?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(9);
    let (family, operation) = tokio::time::timeout_at(deadline, receive)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())??;
    let updates = match tokio::time::timeout_at(deadline, operation.clone()).await {
        Ok(result) => result.map_err(|error| error.to_string())?,
        Err(error) => {
            let _ = operation.Cancel();
            return Err(error.to_string());
        }
    };

    // Optional content must not trigger a notification about the app itself.
    let matches_package = || -> windows::core::Result<bool> {
        for index in 0..updates.Size()? {
            if updates.GetAt(index)?.Package()?.Id()?.FamilyName()? == family {
                return Ok(true);
            }
        }
        Ok(false)
    };
    matches_package().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_store_listing(_app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(all(windows, feature = "store"))]
    {
        use tauri_plugin_opener::OpenerExt;

        _app.opener()
            .open_url(STORE_LISTING, None::<&str>)
            .map_err(|error| {
                tracing::debug!(?error, "Store listing could not be opened");
                "Could not open Microsoft Store. Try opening it from Start.".into()
            })
    }
    #[cfg(not(all(windows, feature = "store")))]
    Err("This installation is not managed by Microsoft Store.".into())
}
