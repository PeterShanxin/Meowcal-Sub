use super::Session;
use crate::inference_health::{Report, CPU_LOCK_EVENT};
use crate::{completion, hy_mt_runtime, protocol::Error};
use serde_json::Value;
use std::time::{Duration, Instant};

trait RecoveryEngine {
    fn lock_cpu(&mut self, progress: &(dyn Fn(String) + Send + Sync));
    async fn sample(&mut self) -> bool;
    async fn start_verified_cpu(
        &mut self,
        progress: &(dyn Fn(String) + Send + Sync),
    ) -> Result<(), Error>;
    fn stop(&mut self);
}

async fn recover(
    engine: &mut impl RecoveryEngine,
    was_gpu: bool,
    repeated: bool,
    progress: &(dyn Fn(String) + Send + Sync),
) -> Result<(), Error> {
    if !repeated && engine.sample().await {
        return Ok(());
    }
    engine.lock_cpu(progress);
    if !was_gpu {
        engine.stop();
        return Err(Error::new(
            "INFERENCE_FAILED",
            "CPU inference check failed; retry the engine explicitly",
        ));
    }
    let result =
        tokio::time::timeout(Duration::from_secs(95), engine.start_verified_cpu(progress)).await;
    match result {
        Ok(Ok(())) => Ok(()),
        Err(_) => {
            // Retain the install lease while blocking work may still be in
            // flight. The fatal readiness code ends Core and its owned engine.
            Err(Error::new(
                "READY_TIMEOUT",
                "CPU recovery exceeded its startup budget",
            ))
        }
        Ok(Err(_)) => {
            engine.stop();
            Err(Error::new(
                "INFERENCE_FAILED",
                "CPU recovery failed; retry the engine explicitly",
            ))
        }
    }
}

impl Session {
    pub(super) async fn recover_inference(
        &mut self,
        report: Report,
        progress: &(dyn Fn(String) + Send + Sync),
    ) -> Result<Value, Error> {
        let Some(repeated) = self.inference.report(&report, Instant::now()) else {
            return Ok(self.status().await);
        };
        if self.endpoint.is_some() {
            // Keep the producing engine's policy even if it exited before the probe.
            let was_gpu = self.gpu;
            recover(self, was_gpu, repeated, progress).await?;
        }
        Ok(self.status().await)
    }
}

impl RecoveryEngine for Session {
    fn lock_cpu(&mut self, progress: &(dyn Fn(String) + Send + Sync)) {
        // The consumer latches before loading so a cancelled or replaced Core
        // cannot silently re-enable GPU in the same application session.
        hy_mt_runtime::lock_cpu();
        progress(CPU_LOCK_EVENT.into());
    }
    async fn sample(&mut self) -> bool {
        let Some(endpoint) = self.endpoint.as_deref() else {
            return false;
        };
        tokio::time::timeout(
            Duration::from_secs(15),
            completion::sample(endpoint, &self.manifest.model.id),
        )
        .await
        .is_ok_and(|result| result.is_ok())
    }
    async fn start_verified_cpu(
        &mut self,
        progress: &(dyn Fn(String) + Send + Sync),
    ) -> Result<(), Error> {
        self.stop();
        self.ready(progress).await
    }
    fn stop(&mut self) {
        Session::stop(self);
    }
}

#[cfg(test)]
#[path = "inference_recovery_tests.rs"]
mod tests;
