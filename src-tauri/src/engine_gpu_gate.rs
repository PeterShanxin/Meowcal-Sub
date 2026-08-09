// =============================================================================
// ENGINE_GPU_GATE.RS - which machines may run the Adreno GPU launch policy
// =============================================================================
// The aarch64 manifest runtime enables full-layer Adreno OpenCL offload
// (`-ngl 99 --no-kv-offload`). The evidence for that policy
// (docs/plans/2026-08-09-adreno-gpu-benchmark.md) was measured on exactly one
// combination: Snapdragon X Elite, Adreno X1-85, Qualcomm OpenCL driver
// 31.0.148.0, llama.cpp b10155. One machine does not prove an architecture,
// so the GPU policy is gated on the validated GPU and everything else keeps
// the previous CPU policy: for this beta, translation on CPU beats GPU
// acceleration that might not start or might wedge the only inference slot.
//
// The gate matches hardware identity, not an exact driver version. An
// exact-driver allowlist would silently flip every future driver update back
// to CPU, and the measured hang is not attributed between llama.cpp and the
// driver - a version boundary would pretend a precision the evidence does not
// have. Same-GPU/driver variation is instead covered by the startup fallback
// in `hy_mt_runtime::ensure_ready` (a GPU launch that never becomes ready is
// retried once on CPU) and, for the mid-session wedge, by issue #103.
//
// Every failure direction is CPU: any enumeration error, a non-Windows host,
// or no matching adapter means "not validated".
// =============================================================================

/// The validated machine's DXGI adapter description is
/// `Qualcomm(R) Adreno(TM) X1-85 GPU`; the `(TM)` sits between the words, so
/// the match is on both tokens rather than one contiguous substring.
const VALIDATED_ADAPTER_TOKENS: &[&str] = &["Adreno", "X1-85"];

/// Whether this machine carries the validated Adreno GPU.
pub(crate) fn validated_adreno_gpu_present() -> bool {
    detect_validated_adapter()
}

#[cfg(target_os = "windows")]
fn matches_validated_adapter(description: &str) -> bool {
    VALIDATED_ADAPTER_TOKENS
        .iter()
        .all(|token| description.contains(token))
}

#[cfg(target_os = "windows")]
fn detect_validated_adapter() -> bool {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    let factory: IDXGIFactory1 = match unsafe { CreateDXGIFactory1() } {
        Ok(factory) => factory,
        Err(_) => return false,
    };
    let mut index = 0u32;
    loop {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(index) }) else {
            return false;
        };
        if let Ok(description) = unsafe { adapter.GetDesc1() }.map(|desc| adapter_name(&desc)) {
            if matches_validated_adapter(&description) {
                return true;
            }
        }
        index += 1;
    }
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

    // The match is on the tokens, exactly as the validated machine reports
    // them - `Adreno(TM) X1-85` has the `(TM)` between the words.
    #[cfg(target_os = "windows")]
    #[test]
    fn matching_tolerates_the_trademark_infix_and_rejects_other_gpus() {
        assert!(matches_validated_adapter(
            "Qualcomm(R) Adreno(TM) X1-85 GPU"
        ));
        assert!(!matches_validated_adapter(
            "Qualcomm(R) Adreno(TM) X1-84 GPU"
        ));
        assert!(!matches_validated_adapter("NVIDIA GeForce RTX 4070"));
        assert!(!matches_validated_adapter("Microsoft Basic Render Driver"));
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
