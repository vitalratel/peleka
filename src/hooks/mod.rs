// ABOUTME: Hooks system for deployment lifecycle events.
// ABOUTME: Discovers and executes shell scripts at pre-deploy, post-deploy, and on-error points.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

use crate::types::ServiceName;

/// Hook execution points in the deployment lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPoint {
    /// Before deployment starts. Failure aborts deployment.
    PreDeploy,
    /// After successful deployment. Failure logs warning.
    PostDeploy,
    /// On deployment failure. Failure logs warning.
    OnError,
}

impl HookPoint {
    /// Get the hook filename for this point.
    pub fn filename(&self) -> &'static str {
        match self {
            HookPoint::PreDeploy => "pre-deploy",
            HookPoint::PostDeploy => "post-deploy",
            HookPoint::OnError => "on-error",
        }
    }

    /// Whether failure at this hook point should abort deployment.
    pub fn is_fatal(&self) -> bool {
        matches!(self, HookPoint::PreDeploy)
    }
}

/// Context passed to hooks via environment variables.
#[derive(Debug, Clone)]
pub struct HookContext {
    pub service: ServiceName,
    pub image: String,
    pub server: String,
    pub runtime: String,
    pub previous_version: Option<String>,
}

impl HookContext {
    /// Convert context to environment variables.
    pub fn to_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert("PELEKA_SERVICE".to_string(), self.service.to_string());
        env.insert("PELEKA_IMAGE".to_string(), self.image.clone());
        env.insert("PELEKA_SERVER".to_string(), self.server.clone());
        env.insert("PELEKA_RUNTIME".to_string(), self.runtime.clone());
        if let Some(ref prev) = self.previous_version {
            env.insert("PELEKA_PREVIOUS_VERSION".to_string(), prev.clone());
        }
        env
    }
}

/// Result of running a hook.
#[derive(Debug)]
pub struct HookResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Discovers and runs hooks from a project directory.
pub struct HookRunner {
    hooks_dir: PathBuf,
}

impl HookRunner {
    /// Create a new hook runner looking for hooks in the given project directory.
    pub fn new(project_dir: &Path) -> Self {
        Self {
            hooks_dir: project_dir.join(".peleka").join("hooks"),
        }
    }

    /// Check if a hook exists for the given point.
    pub fn hook_exists(&self, point: HookPoint) -> bool {
        self.hook_path(point).is_file()
    }

    /// Get the path to a hook script.
    fn hook_path(&self, point: HookPoint) -> PathBuf {
        self.hooks_dir.join(point.filename())
    }

    /// Run a hook if it exists.
    ///
    /// Returns None if the hook doesn't exist, or Some(HookResult) if it was run.
    pub async fn run(&self, point: HookPoint, context: &HookContext) -> Option<HookResult> {
        let hook_path = self.hook_path(point);

        if !hook_path.is_file() {
            return None;
        }

        tracing::info!("Running {} hook: {}", point.filename(), hook_path.display());

        let env_vars = context.to_env();

        let output = Command::new(&hook_path)
            .envs(&env_vars)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match output {
            Ok(output) => {
                let result = HookResult {
                    success: output.status.success(),
                    exit_code: output.status.code(),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                };

                if result.success {
                    tracing::info!("{} hook completed successfully", point.filename());
                } else {
                    tracing::warn!(
                        "{} hook failed with exit code {:?}",
                        point.filename(),
                        result.exit_code
                    );
                }

                Some(result)
            }
            Err(e) => {
                tracing::error!("Failed to execute {} hook: {}", point.filename(), e);
                Some(HookResult {
                    success: false,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: e.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_point_filenames() {
        assert_eq!(HookPoint::PreDeploy.filename(), "pre-deploy");
        assert_eq!(HookPoint::PostDeploy.filename(), "post-deploy");
        assert_eq!(HookPoint::OnError.filename(), "on-error");
    }

    #[test]
    fn pre_deploy_is_fatal() {
        assert!(HookPoint::PreDeploy.is_fatal());
        assert!(!HookPoint::PostDeploy.is_fatal());
        assert!(!HookPoint::OnError.is_fatal());
    }

    #[test]
    fn hook_context_to_env() {
        let context = HookContext {
            service: ServiceName::new("myapp").unwrap(),
            image: "ghcr.io/org/myapp:v1.2.3".to_string(),
            server: "app.example.com".to_string(),
            runtime: "podman".to_string(),
            previous_version: Some("v1.2.2".to_string()),
        };

        let env = context.to_env();
        assert_eq!(env.get("PELEKA_SERVICE"), Some(&"myapp".to_string()));
        assert_eq!(
            env.get("PELEKA_IMAGE"),
            Some(&"ghcr.io/org/myapp:v1.2.3".to_string())
        );
        assert_eq!(
            env.get("PELEKA_SERVER"),
            Some(&"app.example.com".to_string())
        );
        assert_eq!(env.get("PELEKA_RUNTIME"), Some(&"podman".to_string()));
        assert_eq!(
            env.get("PELEKA_PREVIOUS_VERSION"),
            Some(&"v1.2.2".to_string())
        );
    }

    #[test]
    fn hook_context_without_previous_version() {
        let context = HookContext {
            service: ServiceName::new("myapp").unwrap(),
            image: "myapp:latest".to_string(),
            server: "localhost".to_string(),
            runtime: "docker".to_string(),
            previous_version: None,
        };

        let env = context.to_env();
        assert!(!env.contains_key("PELEKA_PREVIOUS_VERSION"));
    }

    #[test]
    fn hook_runner_checks_hooks_dir() {
        let runner = HookRunner::new(Path::new("/nonexistent"));
        assert!(!runner.hook_exists(HookPoint::PreDeploy));
    }
}
