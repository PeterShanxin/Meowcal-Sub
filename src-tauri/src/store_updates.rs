//! MSIX update availability and the handoff to Microsoft Store.

#[cfg(all(windows, feature = "store"))]
const STORE_LISTING: &str = "ms-windows-store://pdp/?ProductId=9NNK2X23VLWT";

#[tauri::command]
pub async fn check_store_update() -> Result<bool, String> {
    #[cfg(all(windows, feature = "store"))]
    {
        let result = tauri::async_runtime::spawn_blocking(check_native).await;
        match result {
            Ok(Ok(available)) => Ok(available),
            Ok(Err(error)) => {
                tracing::debug!(?error, "Store update check failed");
                Err("Cannot check Microsoft Store for updates right now. Try again.".into())
            }
            Err(error) => {
                tracing::debug!(?error, "Store update check worker failed");
                Err("Cannot check Microsoft Store for updates right now. Try again.".into())
            }
        }
    }
    #[cfg(not(all(windows, feature = "store")))]
    Err("This installation is not managed by Microsoft Store.".into())
}

#[cfg(all(windows, feature = "store"))]
fn check_native() -> windows::core::Result<bool> {
    use std::time::Duration;
    use windows::ApplicationModel::Package;
    use windows::Services::Store::StoreContext;
    use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

    // Keep WinRT objects inside the initialized blocking worker's apartment.
    unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
    let _apartment = scopeguard::guard((), |()| unsafe { RoUninitialize() });
    let family = Package::Current()?.Id()?.FamilyName()?;
    let operation = StoreContext::GetDefault()?.GetAppAndOptionalStorePackageUpdatesAsync()?;
    let updates = tauri::async_runtime::block_on(async {
        match tokio::time::timeout(Duration::from_secs(9), operation.clone()).await {
            Ok(result) => result,
            Err(_) => {
                let _ = operation.Cancel();
                Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x800705B4_u32 as i32,
                )))
            }
        }
    })?;
    // Optional content must not trigger a notification about the app itself.
    for index in 0..updates.Size()? {
        if updates.GetAt(index)?.Package()?.Id()?.FamilyName()? == family {
            return Ok(true);
        }
    }
    Ok(false)
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
