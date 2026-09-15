use super::*;
use tokio::time::{sleep, timeout_at};

pub async fn ensure_ready(
    runtime: &ManagedLocalRuntimeConfig,
    timeout: Duration,
    progress: &(dyn Fn(String) + Send + Sync),
) -> Result<String, String> {
    ensure_ready_with_policy(
        runtime,
        Instant::now() + timeout,
        timeout,
        cpu_locked(),
        progress,
    )
    .await
}
/// GPU readiness failure retries on CPU within one deadline; post-ready wedges remain #103.
async fn ensure_ready_with_policy(
    runtime: &ManagedLocalRuntimeConfig,
    deadline: Instant,
    timeout: Duration,
    force_cpu: bool,
    progress: &(dyn Fn(String) + Send + Sync),
) -> Result<String, String> {
    let timeout_error = || {
        format!(
            "HY-MT runtime did not become ready within {} seconds",
            timeout.as_secs()
        )
    };
    if let Some(endpoint) = active_owned_endpoint(runtime) {
        let healthy = timeout_at(deadline, owned_runtime_is_healthy(runtime, &endpoint))
            .await
            .unwrap_or(false);
        if healthy && (!force_cpu || owned_acceleration() == Some("cpu")) {
            return Ok(endpoint);
        }
    }
    if Instant::now() >= deadline {
        return Err(timeout_error());
    }
    let manifest = EngineManifest::shipped().map_err(|error| error.to_string())?;
    let runtime_spec = manifest
        .runtime_for_current_arch()
        .map_err(|error| error.to_string())?;
    let policy = effective_launch_policy(
        runtime_spec,
        !force_cpu && crate::engine_gpu_gate::adreno_gpu_allowed(),
        force_cpu,
    );
    if force_cpu && owned_acceleration() == Some("gpu") {
        shutdown_owned();
    }
    let endpoint = start_with_policy(runtime, &manifest, &policy)?;
    let attempt_started = Instant::now();
    let attempt_deadline = readiness_deadline(deadline, attempt_started, policy.gpu_active);
    let healthy = timeout_at(attempt_deadline, async {
        loop {
            sleep(Duration::from_millis(500)).await;
            if owned_runtime_is_healthy(runtime, &endpoint).await {
                return;
            }
        }
    })
    .await
    .is_ok();
    if !policy.gpu_active {
        return if healthy {
            Ok(endpoint)
        } else {
            Err(timeout_error())
        };
    }
    // A GPU engine can pass health checks while generating corrupt output
    // (#105), so a GPU start counts only once it translates the sample.
    let failure = if healthy {
        let sample_started = Instant::now();
        let sample_deadline = gpu_sample_deadline(deadline, attempt_deadline, sample_started);
        match timeout_at(
            sample_deadline,
            crate::completion::sample(&endpoint, &manifest.model.id),
        )
        .await
        {
            Ok(Ok(())) => return Ok(endpoint),
            Ok(Err(error)) => format!("failed its sample translation ({error})"),
            Err(_) => format!(
                "did not finish its sample translation within {} seconds",
                sample_deadline
                    .saturating_duration_since(sample_started)
                    .as_secs()
            ),
        }
    } else {
        format!(
            "did not become ready within {} seconds",
            attempt_deadline
                .saturating_duration_since(attempt_started)
                .as_secs()
        )
    };
    let _ = writeln!(
        std::io::stderr().lock(),
        "HY-MT GPU engine {failure}; retrying on CPU ({} seconds remaining)",
        deadline.saturating_duration_since(Instant::now()).as_secs()
    );
    lock_cpu();
    progress(crate::inference_health::CPU_LOCK_EVENT.into());
    shutdown_owned();
    Box::pin(ensure_ready_with_policy(
        runtime, deadline, timeout, true, progress,
    ))
    .await
}
