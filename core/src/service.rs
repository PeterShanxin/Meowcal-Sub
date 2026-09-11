use crate::completion::{self, CompletionParams};
use crate::engine_manifest::EngineManifest;
use crate::hy_mt_runtime::{self, HyMtInstallPaths};
use crate::protocol::{Error, Request, API_VERSION, CAPABILITIES, CORE_VERSION};
use crate::storage::{self, Lease};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Hello {
    client: String,
    profile: String,
    expected_version: String,
    storage_root: Option<PathBuf>,
    #[serde(default)]
    legacy_roots: Vec<PathBuf>,
}

struct Session {
    manifest: EngineManifest,
    paths: HyMtInstallPaths,
    legacy_roots: Vec<PathBuf>,
    lease: Option<Lease>,
    endpoint: Option<String>,
}

#[derive(Default)]
pub struct Service {
    session: Option<Session>,
}

impl Service {
    pub async fn handle(
        &mut self,
        request: Request,
        progress: &(dyn Fn(String) + Send + Sync),
    ) -> Result<Value, Error> {
        if request.api != API_VERSION {
            return Err(Error::new("API_MISMATCH", "Unsupported Core API version"));
        }
        if request.method == "hello" {
            return self.hello(request.params);
        }
        if request.method == "shutdown" {
            empty_params(&request.params)?;
            self.shutdown();
            return Ok(json!({"stopped":true}));
        }
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| Error::new("NOT_INITIALIZED", "Call hello before using Core"))?;
        match request.method.as_str() {
            "status" => {
                empty_params(&request.params)?;
                Ok(session.status().await)
            }
            "install" => {
                empty_params(&request.params)?;
                session.stop();
                session.lease =
                    Some(Lease::acquire(&session.paths.root, true, Duration::from_secs(10)).await?);
                let result = tokio::time::timeout(Duration::from_secs(1800), async {
                    storage::import_legacy(
                        &session.paths,
                        &session.legacy_roots,
                        &session.manifest,
                        false,
                    )
                    .await?;
                    crate::installer::install(progress, session.paths.root.clone()).await
                })
                .await;
                hy_mt_runtime::shutdown_owned();
                let result = result.map_err(|_| {
                    Error::new("INSTALL_TIMEOUT", "Installation exceeded 30 minutes")
                })?;
                session.lease = None;
                session.paths = result?;
                Ok(session.status().await)
            }
            "ready" => {
                empty_params(&request.params)?;
                match tokio::time::timeout(Duration::from_secs(110), session.ready(progress)).await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        session.stop();
                        return Err(error);
                    }
                    Err(_) => {
                        return Err(Error::new(
                            "READY_TIMEOUT",
                            "Engine readiness exceeded its startup budget; use install or repair",
                        ))
                    }
                }
                Ok(session.status().await)
            }
            "complete" => {
                let params: CompletionParams =
                    serde_json::from_value(Value::Object(request.params)).map_err(|_| {
                        Error::new("INVALID_COMPLETION", "Invalid completion parameters")
                    })?;
                completion::validate(&params, &session.manifest.model.id)?;
                if !hy_mt_runtime::is_healthy(&session.paths.managed_config(&session.manifest))
                    .await
                {
                    session.stop();
                    return Err(Error::new(
                        "NOT_READY",
                        "Owned runtime is no longer healthy",
                    ));
                }
                let endpoint = session
                    .endpoint
                    .as_deref()
                    .ok_or_else(|| Error::new("NOT_READY", "Call ready before completion"))?;
                completion::execute(endpoint, &params).await
            }
            "ocrLanguages" | "ocrInitialize" | "ocrRecognize" => {
                crate::ocr_service::dispatch(&request.method, &request.params).await
            }
            _ => Err(Error::new("UNKNOWN_METHOD", "Unknown Core method")),
        }
    }

    fn hello(&mut self, params: serde_json::Map<String, Value>) -> Result<Value, Error> {
        if self.session.is_some() {
            return Err(Error::new(
                "ALREADY_INITIALIZED",
                "Core session is already initialized",
            ));
        }
        let hello: Hello = serde_json::from_value(Value::Object(params))
            .map_err(|_| Error::new("INVALID_HELLO", "Invalid Core initialization parameters"))?;
        if hello.expected_version != CORE_VERSION {
            return Err(Error::new(
                "VERSION_MISMATCH",
                "Core version does not match the consumer pin",
            ));
        }
        if !matches!(hello.client.as_str(), "sub1" | "sub2") || hello.legacy_roots.len() > 8 {
            return Err(Error::new(
                "INVALID_HELLO",
                "Unsupported client or legacy root count",
            ));
        }
        for root in &hello.legacy_roots {
            storage::validate_absolute(root)?;
        }
        let root = storage::resolve_root(&hello.profile, hello.storage_root.as_deref())?;
        let manifest = EngineManifest::shipped().map_err(|error| Error::from(error.to_string()))?;
        let runtime = manifest
            .runtime_for_current_arch()
            .map_err(|error| Error::from(error.to_string()))?;
        let paths = HyMtInstallPaths::from_cache_root(root, &manifest, runtime);
        let result = json!({"version":CORE_VERSION,"api":API_VERSION,"capabilities":CAPABILITIES,"model":manifest.model.id,"storageRoot":paths.root});
        self.session = Some(Session {
            manifest,
            paths,
            legacy_roots: hello.legacy_roots,
            lease: None,
            endpoint: None,
        });
        Ok(result)
    }

    pub fn shutdown(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.stop();
        }
        hy_mt_runtime::shutdown_owned();
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Session {
    fn stop(&mut self) {
        hy_mt_runtime::shutdown_owned();
        self.endpoint = None;
        self.lease = None;
    }

    async fn ready(&mut self, progress: &(dyn Fn(String) + Send + Sync)) -> Result<(), Error> {
        let config = self.paths.managed_config(&self.manifest);
        if self.endpoint.is_some() && hy_mt_runtime::is_healthy(&config).await {
            return Ok(());
        }
        self.stop();
        self.lease = Some(Lease::acquire(&self.paths.root, false, Duration::from_secs(10)).await?);
        if storage::verify(&self.paths, &self.manifest).await.is_err() {
            self.lease = None;
            self.lease =
                Some(Lease::acquire(&self.paths.root, true, Duration::from_secs(10)).await?);
            if storage::verify(&self.paths, &self.manifest).await.is_err() {
                let migrated = async {
                    storage::import_legacy(&self.paths, &self.legacy_roots, &self.manifest, true)
                        .await?;
                    crate::installer::install_local(progress, self.paths.root.clone()).await
                }
                .await;
                hy_mt_runtime::shutdown_owned();
                match migrated {
                    Ok(paths) => self.paths = paths,
                    Err(error) => {
                        self.lease = None;
                        return Err(error.into());
                    }
                }
            }
            self.lease = None;
            self.lease =
                Some(Lease::acquire(&self.paths.root, false, Duration::from_secs(10)).await?);
            storage::verify(&self.paths, &self.manifest).await?;
        }
        let result = hy_mt_runtime::ensure_ready(&config, Duration::from_secs(90)).await;
        match result {
            Ok(endpoint) => {
                self.endpoint = Some(endpoint);
                Ok(())
            }
            Err(error) => {
                hy_mt_runtime::shutdown_owned();
                self.lease = None;
                Err(error.into())
            }
        }
    }

    async fn status(&self) -> Value {
        let runtime = self.manifest.runtime_for_current_arch();
        let installed =
            runtime.is_ok_and(|runtime| self.paths.is_complete(&self.manifest, runtime));
        let config = self.paths.managed_config(&self.manifest);
        let ready = self.endpoint.is_some() && hy_mt_runtime::is_healthy(&config).await;
        json!({"installed":installed,"ready":ready,"model":self.manifest.model.id,"version":CORE_VERSION,
            "storageRoot":self.paths.root,"managedConfig":config,"installPaths":self.paths,
            "acceleration":hy_mt_runtime::owned_acceleration()})
    }
}

fn empty_params(params: &serde_json::Map<String, Value>) -> Result<(), Error> {
    if params.is_empty() {
        Ok(())
    } else {
        Err(Error::new(
            "INVALID_PARAMS",
            "Method does not accept parameters",
        ))
    }
}
