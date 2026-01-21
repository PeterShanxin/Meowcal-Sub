// =============================================================================
// D3D.RS - Direct3D11 Device Helpers
// =============================================================================
// This file contains helper functions to create and manage Direct3D11 devices.
// Direct3D11 is needed by the Windows.Graphics.Capture API to efficiently
// capture screen content, especially hardware-accelerated video.
// =============================================================================

use windows::core::{Interface, Result};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::{
    Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP},
    Direct3D11::{
        D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION,
    },
    Dxgi::{IDXGIDevice, DXGI_ERROR_UNSUPPORTED},
};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};

/// Create a D3D11 device with the specified driver type
///
/// # Arguments
/// * `driver_type` - Hardware (GPU) or WARP (software fallback)
/// * `flags` - Creation flags (we use BGRA support for screen capture)
/// * `device` - Output parameter for the created device
fn create_d3d_device_with_type(
    driver_type: D3D_DRIVER_TYPE,
    flags: D3D11_CREATE_DEVICE_FLAG,
    device: *mut Option<ID3D11Device>,
) -> Result<()> {
    unsafe {
        D3D11CreateDevice(
            None,                          // Use default adapter
            driver_type,                   // Hardware or WARP
            HMODULE(std::ptr::null_mut()), // No software rasterizer DLL
            flags,                         // Creation flags
            None,                          // Feature levels (use default)
            D3D11_SDK_VERSION,             // SDK version
            Some(device),                  // Output device
            None,                          // Output feature level
            None,                          // Output immediate context
        )
    }
}

/// Create a Direct3D11 device for screen capture
///
/// This tries to create a hardware-accelerated device first.
/// If that fails (e.g., no GPU available), it falls back to WARP
/// (Windows Advanced Rasterization Platform) which is a software renderer.
///
/// # Returns
/// * `Ok(ID3D11Device)` - The created device
/// * `Err` - If device creation failed
pub fn create_d3d_device() -> Result<ID3D11Device> {
    let mut device = None;

    // First, try to create a hardware (GPU) device
    let mut result = create_d3d_device_with_type(
        D3D_DRIVER_TYPE_HARDWARE,
        D3D11_CREATE_DEVICE_BGRA_SUPPORT, // Required for screen capture
        &mut device,
    );

    // If hardware device creation failed because it's unsupported,
    // fall back to WARP (software rendering)
    if let Err(error) = &result {
        if error.code() == DXGI_ERROR_UNSUPPORTED {
            tracing::warn!("Hardware D3D11 not available, falling back to WARP");
            result = create_d3d_device_with_type(
                D3D_DRIVER_TYPE_WARP,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                &mut device,
            );
        }
    }

    result?;
    Ok(device.unwrap())
}

/// Wrap a D3D11 device as a WinRT IDirect3DDevice
///
/// The Windows.Graphics.Capture API expects a WinRT IDirect3DDevice,
/// but we have a Win32 ID3D11Device. This function bridges the two.
///
/// # Arguments
/// * `d3d_device` - The Win32 D3D11 device
///
/// # Returns
/// * `Ok(IDirect3DDevice)` - The WinRT wrapper
pub fn create_direct3d_device(d3d_device: &ID3D11Device) -> Result<IDirect3DDevice> {
    // Cast to DXGI device (the underlying interface)
    let dxgi_device: IDXGIDevice = d3d_device.cast()?;

    // Create the WinRT wrapper
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device)? };

    // Cast to IDirect3DDevice
    inspectable.cast()
}

/// Extract a D3D interface from a WinRT object
///
/// This is the reverse of create_direct3d_device - it extracts the underlying
/// D3D11 interface from a WinRT Direct3D object (like a capture frame surface).
///
/// # Type Parameters
/// * `S` - The source WinRT type
/// * `R` - The target D3D11 type to extract
pub fn get_d3d_interface_from_object<S: Interface, R: Interface>(object: &S) -> Result<R> {
    // Get the interop interface that provides access to the underlying D3D object
    let access: IDirect3DDxgiInterfaceAccess = object.cast()?;

    // Get the actual D3D interface
    let object = unsafe { access.GetInterface::<R>()? };
    Ok(object)
}
