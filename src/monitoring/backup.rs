use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

use crate::db::Database;

/// Periodically backs up the SQLite database to a timestamped file.
pub struct BackupTask {
    db: Arc<dyn Database>,
    backup_dir: String,
    interval: Duration,
}

impl BackupTask {
    pub fn new(db: Arc<dyn Database>, backup_dir: String, interval_secs: u64) -> Self {
        Self {
            db,
            backup_dir,
            interval: Duration::from_secs(interval_secs),
        }
    }

    pub async fn run(self) {
        if let Err(e) = std::fs::create_dir_all(&self.backup_dir) {
            error!(error = %e, dir = %self.backup_dir, "Failed to create backup directory");
            return;
        }

        info!(interval_secs = self.interval.as_secs(), dir = %self.backup_dir, "DB backup task started");
        let mut interval = tokio::time::interval(self.interval);

        loop {
            interval.tick().await;

            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            // Canonicalize the backup directory to prevent path traversal. The timestamp
            // is generated internally so only the directory needs validation.
            let canon_dir = match std::fs::canonicalize(&self.backup_dir) {
                Ok(p) => p,
                Err(e) => {
                    error!(error = %e, dir = %self.backup_dir, "Backup dir canonicalization failed");
                    continue;
                }
            };
            let canon_str = canon_dir.to_string_lossy();
            let dest = format!("{}/mercury_backup_{}.db", canon_str, timestamp);

            match self.db.backup_to_file(&dest).await {
                Ok(()) => {
                    info!(path = %dest, "Database backup completed");
                    // Prune old backups: keep last 7
                    if let Err(e) = self.prune_old_backups(7) {
                        warn!(error = %e, "Failed to prune old backups");
                    }
                }
                Err(e) => {
                    error!(error = %e, "Database backup failed");
                }
            }
        }
    }

    fn prune_old_backups(&self, keep: usize) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.backup_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.starts_with("mercury_backup_") && n.ends_with(".db"))
                    .unwrap_or(false)
            })
            .collect();

        // L-9 FIX: Sort by actual file creation/modification time, not lexicographically
        entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH));

        if entries.len() > keep {
            for entry in &entries[..entries.len() - keep] {
                std::fs::remove_file(entry.path())?;
                info!(path = ?entry.path(), "Pruned old backup");
            }
        }
        Ok(())
    }
}