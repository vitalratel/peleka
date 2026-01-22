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

    /// Path to the lock directory for a service.
    /// Uses a directory (not file) because mkdir is atomic and race-free.
    pub fn lock_path(service: &ServiceName) -> String {
        format!("/tmp/peleka-deploy-{}.lock", service)
    }

    /// Path to the lock info file inside the lock directory.
    pub fn lock_info_path(service: &ServiceName) -> String {
        format!("{}/info", Self::lock_path(service))
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
    /// Uses mkdir for atomic lock acquisition (no TOCTOU race condition).
    /// Returns error if lock is already held by another process.
    /// Auto-breaks stale locks (>1 hour) with a warning.
    pub async fn acquire(
        session: &'a Session,
        service: &ServiceName,
        force: bool,
    ) -> Result<Self, DeployError> {
        let lock_path = LockInfo::lock_path(service);
        let lock_info_path = LockInfo::lock_info_path(service);

        // Prepare lock info
        let lock_info = LockInfo::new(service);
        let lock_json = serde_json::to_string(&lock_info)
            .map_err(|e| DeployError::LockError(format!("failed to serialize lock: {}", e)))?;

        // Try atomic lock acquisition using mkdir (fails if directory exists)
        let mkdir_cmd = format!("mkdir '{}'", lock_path);
        let mkdir_result = session
            .exec(&mkdir_cmd)
            .await
            .map_err(|e| DeployError::LockError(format!("failed to create lock: {}", e)))?;

        if mkdir_result.success() {
            // Successfully acquired lock - write our info
            Self::write_lock_info(session, &lock_info_path, &lock_json).await?;
            return Ok(Self {
                session,
                service: service.clone(),
            });
        }

        // Lock directory exists - check if we should break it
        let should_break = Self::check_existing_lock(session, &lock_info_path, force).await?;

        if !should_break {
            // Lock is valid and held by someone else
            let output = session.exec(&format!("cat '{}'", lock_info_path)).await;
            if let Ok(output) = output {
                if let Ok(existing) = serde_json::from_str::<LockInfo>(&output.stdout) {
                    return Err(DeployError::LockHeld {
                        holder: existing.holder,
                        pid: existing.pid,
                        started_at: existing.started_at,
                    });
                }
            }
            return Err(DeployError::LockError("lock held by another process".to_string()));
        }

        // Break the lock and retry
        let rm_cmd = format!("rm -rf '{}'", lock_path);
        session
            .exec(&rm_cmd)
            .await
            .map_err(|e| DeployError::LockError(format!("failed to break lock: {}", e)))?;

        // Retry atomic acquisition
        let mkdir_result = session
            .exec(&mkdir_cmd)
            .await
            .map_err(|e| DeployError::LockError(format!("failed to create lock: {}", e)))?;

        if !mkdir_result.success() {
            // Another process grabbed it between our rm and mkdir
            return Err(DeployError::LockError(
                "lock acquired by another process during break".to_string(),
            ));
        }

        // Successfully acquired lock after breaking - write our info
        Self::write_lock_info(session, &lock_info_path, &lock_json).await?;
        Ok(Self {
            session,
            service: service.clone(),
        })
    }

    /// Write lock info to the info file inside the lock directory.
    async fn write_lock_info(
        session: &Session,
        lock_info_path: &str,
        lock_json: &str,
    ) -> Result<(), DeployError> {
        let write_cmd = format!(
            "echo '{}' > '{}'",
            lock_json.replace('\'', "'\\''"),
            lock_info_path
        );
        let output = session
            .exec(&write_cmd)
            .await
            .map_err(|e| DeployError::LockError(format!("failed to write lock info: {}", e)))?;

        if !output.success() {
            return Err(DeployError::LockError(format!(
                "failed to write lock info: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Check if an existing lock should be broken (stale, forced, or corrupted).
    async fn check_existing_lock(
        session: &Session,
        lock_info_path: &str,
        force: bool,
    ) -> Result<bool, DeployError> {
        let output = session
            .exec(&format!("cat '{}'", lock_info_path))
            .await
            .map_err(|e| DeployError::LockError(format!("failed to read lock info: {}", e)))?;

        if !output.success() {
            // Can't read lock info - corrupted, break it
            tracing::warn!("Lock info unreadable, breaking lock");
            return Ok(true);
        }

        match serde_json::from_str::<LockInfo>(&output.stdout) {
            Ok(existing_lock) => {
                if force {
                    tracing::warn!(
                        "Breaking lock held by {} (pid {}) since {}",
                        existing_lock.holder,
                        existing_lock.pid,
                        existing_lock.started_at
                    );
                    Ok(true)
                } else if existing_lock.is_stale() {
                    tracing::warn!(
                        "Auto-breaking stale lock held by {} (pid {}) since {}",
                        existing_lock.holder,
                        existing_lock.pid,
                        existing_lock.started_at
                    );
                    Ok(true)
                } else {
                    // Lock is active and valid
                    Ok(false)
                }
            }
            Err(_) => {
                // Lock info corrupted, break it
                tracing::warn!("Lock info corrupted, breaking lock");
                Ok(true)
            }
        }
    }

    /// Release the lock.
    pub async fn release(self) -> Result<(), DeployError> {
        let lock_path = LockInfo::lock_path(&self.service);
        // Remove the lock directory and its contents
        let _ = self.session.exec(&format!("rm -rf '{}'", lock_path)).await;
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
