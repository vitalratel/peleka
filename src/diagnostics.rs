// ABOUTME: Diagnostics accumulator for non-fatal warnings during deployment.
// ABOUTME: Collects warnings that shouldn't fail a deployment but should be shown to users.

/// Collects non-fatal warnings during deployment operations.
#[derive(Default)]
pub struct Diagnostics {
    warnings: Vec<Warning>,
}

impl Diagnostics {
    /// Record a warning, auto-logging it via tracing.
    pub fn warn(&mut self, warning: Warning) {
        tracing::warn!("{}", warning.message);
        self.warnings.push(warning);
    }

    /// Get all collected warnings.
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Check if any warnings were collected.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

/// A non-fatal warning collected during deployment.
#[derive(Debug, Clone)]
pub struct Warning {
    pub kind: WarningKind,
    pub message: String,
}

impl Warning {
    /// Create a lock release warning.
    pub fn lock_release(message: impl Into<String>) -> Self {
        Self {
            kind: WarningKind::LockRelease,
            message: message.into(),
        }
    }

    /// Create an SSH disconnect warning.
    pub fn ssh_disconnect(message: impl Into<String>) -> Self {
        Self {
            kind: WarningKind::SshDisconnect,
            message: message.into(),
        }
    }
}

/// Categories of warnings that can occur during deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningKind {
    /// Failed to release deploy lock (lock file may remain).
    LockRelease,
    /// Failed to cleanly disconnect SSH session.
    SshDisconnect,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_starts_empty() {
        let diag = Diagnostics::default();
        assert!(!diag.has_warnings());
        assert!(diag.warnings().is_empty());
    }

    #[test]
    fn diagnostics_collects_warnings() {
        let mut diag = Diagnostics::default();

        diag.warn(Warning::lock_release("failed to remove lock file"));
        diag.warn(Warning::ssh_disconnect("connection reset"));

        assert!(diag.has_warnings());
        assert_eq!(diag.warnings().len(), 2);
    }

    #[test]
    fn warning_constructors_set_correct_kind() {
        let lock_warning = Warning::lock_release("test");
        assert_eq!(lock_warning.kind, WarningKind::LockRelease);

        let ssh_warning = Warning::ssh_disconnect("test");
        assert_eq!(ssh_warning.kind, WarningKind::SshDisconnect);
    }
}
