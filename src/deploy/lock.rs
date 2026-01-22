// ABOUTME: Deploy lock to prevent concurrent deployments to the same service.
// ABOUTME: Uses atomic file creation with lock info stored in ~/.local/state/peleka/.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ssh::Session;
use crate::types::ServiceName;

use super::DeployError;

/// Base directory for peleka state files (XDG Base Directory compliant).
const STATE_DIR: &str = ".local/state/peleka";

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
    /// Uses $HOME for shell expansion compatibility.
    pub fn lock_path(service: &ServiceName) -> String {
        format!("$HOME/{}/{}.lock", STATE_DIR, service)
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
    /// Uses shell noclobber mode for atomic lock acquisition (no TOCTOU race).
    /// Returns error if lock is already held by another process.
    /// Auto-breaks stale locks (>1 hour) with a warning.
    pub async fn acquire(
        session: &'a Session,
        service: &ServiceName,
        force: bool,
    ) -> Result<Self, DeployError> {
        let lock_path = LockInfo::lock_path(service);

        // Ensure state directory exists
        Self::ensure_state_dir(session).await?;

        // Prepare lock info
        let lock_info = LockInfo::new(service);
        let lock_json = serde_json::to_string(&lock_info)
            .map_err(|e| DeployError::lock_error(format!("failed to serialize lock: {}", e)))?;
        let escaped_json = lock_json.replace('\'', "'\\''");

        // Try atomic lock acquisition using noclobber mode
        // set -C makes > fail if file already exists (atomic create-if-not-exists)
        // Use double quotes for path to expand $HOME, single quotes for JSON
        let acquire_cmd = format!(
            "(set -C; echo '{}' > \"{}\") 2>/dev/null",
            escaped_json, lock_path
        );

        let result = session
            .exec(&acquire_cmd)
            .await
            .map_err(|e| DeployError::lock_error(format!("failed to acquire lock: {}", e)))?;

        if result.success() {
            return Ok(Self {
                session,
                service: service.clone(),
            });
        }

        // Lock acquisition failed - check if existing lock should be broken
        let should_break = Self::check_existing_lock(session, &lock_path, force).await?;

        if !should_break {
            // Lock is valid and held by someone else
            let output = session.exec(&format!("cat \"{}\"", lock_path)).await;
            if let Ok(output) = output
                && let Ok(existing) = serde_json::from_str::<LockInfo>(&output.stdout)
            {
                return Err(DeployError::lock_held(
                    existing.holder,
                    existing.pid,
                    existing.started_at,
                ));
            }
            return Err(DeployError::lock_error(
                "lock held by another process".to_string(),
            ));
        }

        // Break the lock and retry
        tracing::debug!("Removing stale/forced lock at {}", lock_path);
        let _ = session.exec(&format!("rm -f \"{}\"", lock_path)).await;

        // Retry acquisition
        let result = session
            .exec(&acquire_cmd)
            .await
            .map_err(|e| DeployError::lock_error(format!("failed to acquire lock: {}", e)))?;

        if !result.success() {
            return Err(DeployError::lock_error(
                "lock acquired by another process during break".to_string(),
            ));
        }

        Ok(Self {
            session,
            service: service.clone(),
        })
    }

    /// Ensure the state directory exists on the remote server.
    async fn ensure_state_dir(session: &Session) -> Result<(), DeployError> {
        let cmd = format!("mkdir -p ~/{}", STATE_DIR);
        let output = session.exec(&cmd).await.map_err(|e| {
            DeployError::lock_error(format!("failed to create state directory: {}", e))
        })?;

        if !output.success() {
            return Err(DeployError::lock_error(format!(
                "failed to create state directory: {}",
                output.stderr
            )));
        }
        Ok(())
    }

    /// Check if an existing lock should be broken (stale, forced, or corrupted).
    async fn check_existing_lock(
        session: &Session,
        lock_path: &str,
        force: bool,
    ) -> Result<bool, DeployError> {
        let output = session
            .exec(&format!("cat \"{}\"", lock_path))
            .await
            .map_err(|e| DeployError::lock_error(format!("failed to read lock info: {}", e)))?;

        if !output.success() {
            // Can't read lock info - corrupted or doesn't exist, break it
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
        let _ = self.session.exec(&format!("rm -f \"{}\"", lock_path)).await;
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
    fn lock_path_uses_state_dir() {
        let service = ServiceName::new("myapp").unwrap();
        assert_eq!(
            LockInfo::lock_path(&service),
            "$HOME/.local/state/peleka/myapp.lock"
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
