// =============================================================================
// ENGINE_GPU_GATE.RS - which machines may run the Adreno GPU launch policy
// =============================================================================
// The aarch64 manifest runtime enables full-layer Adreno OpenCL offload
// (`-ngl 99 --no-kv-offload`). The evidence for that policy
// (docs/plans/2026-08-09-adreno-gpu-benchmark.md) was measured on exactly one
// combination: Snapdragon X Elite, Adreno X1-85, Qualcomm OpenCL driver
// 31.0.148.0, llama.cpp b10155. One machine does not prove an architecture or
// a future driver, so the GPU policy is gated on that GPU + driver version and
// everything else keeps the previous CPU policy. A driver update deliberately
// returns to CPU until measured evidence expands the allowlist: translation on
// CPU beats GPU acceleration that might wedge the only inference slot.
//
// Every failure direction is CPU: any enumeration error, a non-Windows host,
// or no matching adapter means "not validated".
// =============================================================================

/// The validated machine's DXGI adapter description is
/// `Qualcomm(R) Adreno(TM) X1-85 GPU`; the `(TM)` sits between the words, so
/// the match is on both tokens rather than one contiguous substring.
const VALIDATED_ADAPTER_TOKENS: &[&str] = &["Adreno", "X1-85"];
const VALIDATED_DRIVER_VERSION: [u16; 4] = [31, 0, 148, 0];

/// Whether this machine carries the validated Adreno GPU + driver combination.
pub(crate) fn validated_adreno_gpu_present() -> bool {
    detect_validated_adapter()
}

#[cfg(target_os = "windows")]
fn matches_validated_adapter(description: &str, driver_version: [u16; 4]) -> bool {
    VALIDATED_ADAPTER_TOKENS
        .iter()
        .all(|token| description.contains(token))
        && driver_version == VALIDATED_DRIVER_VERSION
}

#[cfg(target_os = "windows")]
fn detect_validated_adapter() -> bool {
    use windows::Win32::Graphics::DXCore::{
        DXCoreCreateAdapterFactory, IDXCoreAdapter, IDXCoreAdapterFactory,
    };
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    let factory: IDXGIFactory1 = match unsafe { CreateDXGIFactory1() } {
        Ok(factory) => factory,
        Err(_) => return false,
    };
    let driver_factory: IDXCoreAdapterFactory = match unsafe { DXCoreCreateAdapterFactory() } {
        Ok(factory) => factory,
        Err(_) => return false,
    };
    let mut index = 0u32;
    loop {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(index) }) else {
            return false;
        };
        if let Ok(desc) = unsafe { adapter.GetDesc1() } {
            let description = adapter_name(&desc);
            if VALIDATED_ADAPTER_TOKENS
                .iter()
                .all(|token| description.contains(token))
            {
                let driver_adapter: IDXCoreAdapter =
                    match unsafe { driver_factory.GetAdapterByLuid(&desc.AdapterLuid) } {
                        Ok(adapter) => adapter,
                        Err(_) => return false,
                    };
                let Some(driver_version) = adapter_driver_version(&driver_adapter) else {
                    return false;
                };
                return matches_validated_adapter(&description, driver_version);
            }
        }
        index += 1;
    }
}

#[cfg(target_os = "windows")]
fn adapter_driver_version(
    adapter: &windows::Win32::Graphics::DXCore::IDXCoreAdapter,
) -> Option<[u16; 4]> {
    use std::mem::size_of;
    use windows::Win32::Graphics::DXCore::DriverVersion;

    if !unsafe { adapter.IsPropertySupported(DriverVersion) }
        || unsafe { adapter.GetPropertySize(DriverVersion) }.ok()? != size_of::<u64>()
    {
        return None;
    }
    let mut raw_version = 0u64;
    unsafe {
        adapter
            .GetProperty(
                DriverVersion,
                size_of::<u64>(),
                (&mut raw_version as *mut u64).cast(),
            )
            .ok()?;
    }
    Some(decode_driver_version(raw_version))
}

/// DXCore returns the four dotted driver-version components as WORDs packed
/// from most significant to least significant in one 64-bit value.
#[cfg(target_os = "windows")]
fn decode_driver_version(raw: u64) -> [u16; 4] {
    [
        (raw >> 48) as u16,
        (raw >> 32) as u16,
        (raw >> 16) as u16,
        raw as u16,
    ]
}

#[cfg(target_os = "windows")]
fn adapter_name(desc: &windows::Win32::Graphics::Dxgi::DXGI_ADAPTER_DESC1) -> String {
    let end = desc
        .Description
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(desc.Description.len());
    String::from_utf16_lossy(&desc.Description[..end])
}

#[cfg(not(target_os = "windows"))]
fn detect_validated_adapter() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // The detector must never panic and must answer on any host; the value
    // itself depends on the machine.
    #[test]
    fn detection_is_a_total_function() {
        let _ = validated_adreno_gpu_present();
    }

    // Name alone is insufficient: a new or old driver has not passed the
    // sustained benchmark and must fail safe to CPU.
    #[cfg(target_os = "windows")]
    #[test]
    fn matching_requires_the_validated_gpu_and_driver() {
        assert!(matches_validated_adapter(
            "Qualcomm(R) Adreno(TM) X1-85 GPU",
            [31, 0, 148, 0]
        ));
        assert!(!matches_validated_adapter(
            "Qualcomm(R) Adreno(TM) X1-85 GPU",
            [31, 0, 149, 0]
        ));
        assert!(!matches_validated_adapter(
            "Qualcomm(R) Adreno(TM) X1-84 GPU",
            [31, 0, 148, 0]
        ));
        assert!(!matches_validated_adapter(
            "NVIDIA GeForce RTX 4070",
            [31, 0, 148, 0]
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn driver_version_words_decode_in_display_order() {
        let raw = (31u64 << 48) | (148u64 << 16);
        assert_eq!(decode_driver_version(raw), [31, 0, 148, 0]);
    }

    // Run explicitly on the validated machine (the benchmark host): the gate
    // must find its Adreno X1-85 or every user of that machine silently loses
    // the GPU policy.
    #[test]
    #[ignore = "asserts this machine IS the validated Adreno X1-85 host"]
    fn the_validated_machine_is_detected() {
        assert!(validated_adreno_gpu_present());
    }
}
