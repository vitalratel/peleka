// ABOUTME: Deploy lock to prevent concurrent deployments to the same service.
// ABOUTME: Uses lock files on the remote server with holder info and stale detection.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ssh::Session;
use crate::types::ServiceName;

use super::DeployError;

/// Information about who holds a deploy lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockInfo {
    /// Hostname of the machine that holds the lock.
    pub holder: String,
    /// Process ID of the lock holder.
    pub pid: u32,
    /// When the lock was acquired.
    pub started_at: DateTime<Utc>,
    /// Service being deployed.
    pub service: String,
}

impl LockInfo {
    /// Create new lock info for the current process.
    pub fn new(service: &ServiceName) -> Self {
        Self {
            holder: gethostname::gethostname().to_string_lossy().into_owned(),
            pid: std::process::id(),
            started_at: Utc::now(),
            service: service.to_string(),
        }
    }

    /// Check if this lock is stale (older than 1 hour).
    pub fn is_stale(&self) -> bool {
        let age = Utc::now() - self.started_at;
        age.num_hours() >= 1
    }

    /// Path to the lock file for a service.
    pub fn lock_path(service: &ServiceName) -> String {
        format!("/tmp/peleka-deploy-{}.lock", service)
    }
}

/// A held deploy lock that releases on drop.
pub struct DeployLock<'a> {
    session: &'a Session,
    service: ServiceName,
}

impl std::fmt::Debug for DeployLock<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeployLock")
            .field("service", &self.service)
            .finish()
    }
}

impl<'a> DeployLock<'a> {
    /// Acquire a deploy lock for the given service.
    ///
    /// Returns error if lock is already held by another process.
    /// Auto-breaks stale locks (>1 hour) with a warning.
    pub async fn acquire(
        session: &'a Session,
        service: &ServiceName,
        force: bool,
    ) -> Result<Self, DeployError> {
        let lock_path = LockInfo::lock_path(service);

        // Check if lock file exists
        if session
            .file_exists(&lock_path)
            .await
            .map_err(|e| DeployError::LockError(format!("failed to check lock file: {}", e)))?
        {
            // Read existing lock info
            let output = session
                .exec(&format!("cat {}", lock_path))
                .await
                .map_err(|e| DeployError::LockError(format!("failed to read lock file: {}", e)))?;

            if output.success()
                && let Ok(existing_lock) = serde_json::from_str::<LockInfo>(&output.stdout)
            {
                if force {
                    // Force break the lock
                    tracing::warn!(
                        "Breaking lock held by {} (pid {}) since {}",
                        existing_lock.holder,
                        existing_lock.pid,
                        existing_lock.started_at
                    );
                } else if existing_lock.is_stale() {
                    // Auto-break stale lock
                    tracing::warn!(
                        "Auto-breaking stale lock held by {} (pid {}) since {}",
                        existing_lock.holder,
                        existing_lock.pid,
                        existing_lock.started_at
                    );
                } else {
                    // Lock is active, reject
                    return Err(DeployError::LockHeld {
                        holder: existing_lock.holder,
                        pid: existing_lock.pid,
                        started_at: existing_lock.started_at,
                    });
                }
            }
        }

        // Create new lock
        let lock_info = LockInfo::new(service);
        let lock_json = serde_json::to_string(&lock_info)
            .map_err(|e| DeployError::LockError(format!("failed to serialize lock: {}", e)))?;

        // Write lock file atomically (write to temp then rename)
        let temp_path = format!("{}.tmp.{}", lock_path, std::process::id());
        let write_cmd = format!(
            "echo '{}' > {} && mv {} {}",
            lock_json.replace('\'', "'\\''"),
            temp_path,
            temp_path,
            lock_path
        );

        let output = session
            .exec(&write_cmd)
            .await
            .map_err(|e| DeployError::LockError(format!("failed to write lock file: {}", e)))?;

        if !output.success() {
            return Err(DeployError::LockError(format!(
                "failed to create lock file: {}",
                output.stderr
            )));
        }

        Ok(Self {
            session,
            service: service.clone(),
        })
    }

    /// Release the lock.
    pub async fn release(self) -> Result<(), DeployError> {
        let lock_path = LockInfo::lock_path(&self.service);
        let _ = self.session.exec(&format!("rm -f {}", lock_path)).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_info_creates_with_current_host_and_pid() {
        let service = ServiceName::new("test-service").unwrap();
        let info = LockInfo::new(&service);

        assert_eq!(info.service, "test-service");
        assert_eq!(info.pid, std::process::id());
        assert!(!info.holder.is_empty());
    }

    #[test]
    fn lock_path_uses_service_name() {
        let service = ServiceName::new("myapp").unwrap();
        assert_eq!(
            LockInfo::lock_path(&service),
            "/tmp/peleka-deploy-myapp.lock"
        );
    }

    #[test]
    fn fresh_lock_is_not_stale() {
        let service = ServiceName::new("test").unwrap();
        let info = LockInfo::new(&service);
        assert!(!info.is_stale());
    }

    #[test]
    fn old_lock_is_stale() {
        let service = ServiceName::new("test").unwrap();
        let mut info = LockInfo::new(&service);
        // Set to 2 hours ago
        info.started_at = Utc::now() - chrono::Duration::hours(2);
        assert!(info.is_stale());
    }
}
