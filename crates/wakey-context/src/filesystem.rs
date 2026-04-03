//! Filesystem layer for Wakey context storage.
//!
//! Implements the OpenViking filesystem paradigm with `wakey://` URIs.
//! The filesystem is the source of truth; SQLite index is rebuilt from it.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, instrument};

use wakey_types::WakeyResult;

/// URI scheme for Wakey context resources.
pub const URI_SCHEME: &str = "wakey://";

/// Preset directories created on first initialization.
pub const PRESET_DIRS: &[&str] = &[
    "user/memories",
    "agent/skills",
    "agent/memories",
    "session",
    "resources",
];

/// A context path represented as a Wakey URI.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContextPath {
    scope: String,
    path: String,
}

impl ContextPath {
    /// Create a new context path from a scope-relative path.
    pub fn new(scope_path: &str) -> Self {
        let parts: Vec<&str> = scope_path.splitn(2, '/').collect();
        let scope = parts.first().copied().unwrap_or("session").to_string();
        let path = parts.get(1).copied().unwrap_or("").to_string();
        Self { scope, path }
    }

    /// Create a context path from a full URI.
    pub fn from_uri(uri: &str) -> Self {
        let stripped = uri.strip_prefix(URI_SCHEME).unwrap_or(uri);
        Self::new(stripped)
    }

    /// Get the URI representation.
    pub fn uri(&self) -> String {
        if self.path.is_empty() {
            format!("{}{}", URI_SCHEME, self.scope)
        } else {
            format!("{}{}/{}", URI_SCHEME, self.scope, self.path)
        }
    }

    /// Get the scope (user, agent, session, resources).
    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// Get the path within the scope.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Get the full scope-relative path.
    pub fn scope_path(&self) -> String {
        if self.path.is_empty() {
            self.scope.clone()
        } else {
            format!("{}/{}", self.scope, self.path)
        }
    }

    /// Get the parent directory path.
    pub fn parent(&self) -> Option<Self> {
        let path_parts: Vec<&str> = self.path.rsplitn(2, '/').collect();
        if path_parts.len() < 2 {
            return None;
        }
        Some(Self {
            scope: self.scope.clone(),
            path: path_parts[1].to_string(),
        })
    }

    /// Check if this path points to a directory (no extension).
    pub fn is_dir(&self) -> bool {
        self.path.ends_with('/') || !self.path.contains('.')
    }

    /// Get the file name (last component).
    pub fn file_name(&self) -> Option<&str> {
        self.path.rsplit('/').next()
    }
}

impl std::fmt::Display for ContextPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.uri())
    }
}

/// Metadata for a context entry (file or directory).
#[derive(Debug, Clone)]
pub struct ContextEntry {
    path: ContextPath,
    is_dir: bool,
    size: u64,
    mtime: i64,
}

impl ContextEntry {
    pub fn path(&self) -> &ContextPath {
        &self.path
    }

    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn mtime(&self) -> i64 {
        self.mtime
    }
}

/// Filesystem backend for context storage.
pub struct ContextFs {
    base_dir: Arc<RwLock<PathBuf>>,
    initialized: Arc<RwLock<bool>>,
}

impl ContextFs {
    /// Create a new ContextFs with the given base directory.
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir: Arc::new(RwLock::new(base_dir)),
            initialized: Arc::new(RwLock::new(false)),
        }
    }

    /// Initialize the filesystem, creating preset directories.
    #[instrument(skip(self))]
    pub async fn initialize(&self) -> WakeyResult<()> {
        let initialized = *self.initialized.read().await;
        if initialized {
            return Ok(());
        }

        let base_dir = self.base_dir.read().await.clone();

        if !base_dir.exists() {
            fs::create_dir_all(&base_dir).await?;
            debug!("Created base directory: {}", base_dir.display());
        }

        for preset in PRESET_DIRS {
            let dir_path = base_dir.join(preset);
            if !dir_path.exists() {
                fs::create_dir_all(&dir_path).await?;
                debug!("Created preset directory: {}", dir_path.display());
            }
        }

        *self.initialized.write().await = true;
        Ok(())
    }

    /// Convert a ContextPath to a physical filesystem path.
    async fn to_fs_path(&self, path: &ContextPath) -> PathBuf {
        let base_dir = self.base_dir.read().await.clone();
        base_dir.join(path.scope()).join(path.path())
    }

    /// Read file content at the given path.
    #[instrument(skip(self))]
    pub async fn read(&self, path: &ContextPath) -> WakeyResult<String> {
        self.initialize().await?;

        let fs_path = self.to_fs_path(path).await;
        let content = fs::read_to_string(&fs_path).await?;
        debug!("Read {} bytes from {}", content.len(), path.uri());
        Ok(content)
    }

    /// Write content to a file at the given path.
    #[instrument(skip(self, content))]
    pub async fn write(&self, path: &ContextPath, content: &str) -> WakeyResult<()> {
        self.initialize().await?;

        let fs_path = self.to_fs_path(path).await;

        if let Some(parent) = fs_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent).await?;
        }

        fs::write(&fs_path, content).await?;
        debug!("Wrote {} bytes to {}", content.len(), path.uri());
        Ok(())
    }

    /// List entries in a directory.
    #[instrument(skip(self))]
    pub async fn list(&self, path: &ContextPath) -> WakeyResult<Vec<ContextEntry>> {
        self.initialize().await?;

        let fs_path = self.to_fs_path(path).await;

        if !fs_path.exists() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(&fs_path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            let name = entry.file_name().to_string_lossy().to_string();

            let entry_path = if path.path().is_empty() {
                ContextPath::new(&format!("{}/{}", path.scope(), name))
            } else {
                ContextPath::new(&format!("{}/{}/{}", path.scope(), path.path(), name))
            };

            entries.push(ContextEntry {
                path: entry_path,
                is_dir: metadata.is_dir(),
                size: metadata.len(),
                mtime: metadata
                    .modified()
                    .map(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64
                    })
                    .unwrap_or(0),
            });
        }

        debug!("Listed {} entries in {}", entries.len(), path.uri());
        Ok(entries)
    }

    /// Delete a file or directory at the given path.
    #[instrument(skip(self))]
    pub async fn delete(&self, path: &ContextPath) -> WakeyResult<bool> {
        self.initialize().await?;

        let fs_path = self.to_fs_path(path).await;

        if !fs_path.exists() {
            return Ok(false);
        }

        if fs_path.is_dir() {
            fs::remove_dir_all(&fs_path).await?;
        } else {
            fs::remove_file(&fs_path).await?;
        }

        debug!("Deleted {}", path.uri());
        Ok(true)
    }

    /// Check if a file or directory exists.
    #[instrument(skip(self))]
    pub async fn exists(&self, path: &ContextPath) -> WakeyResult<bool> {
        self.initialize().await?;

        let fs_path = self.to_fs_path(path).await;
        Ok(fs_path.exists())
    }

    /// Get metadata for a file or directory.
    #[instrument(skip(self))]
    pub async fn metadata(&self, path: &ContextPath) -> WakeyResult<Option<ContextEntry>> {
        self.initialize().await?;

        let fs_path = self.to_fs_path(path).await;

        if !fs_path.exists() {
            return Ok(None);
        }

        let metadata = fs::metadata(&fs_path).await?;

        Ok(Some(ContextEntry {
            path: path.clone(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            mtime: metadata
                .modified()
                .map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64
                })
                .unwrap_or(0),
        }))
    }

    /// Recursively list all files under a directory.
    #[instrument(skip(self))]
    pub async fn list_all_files(&self, path: &ContextPath) -> WakeyResult<Vec<ContextEntry>> {
        self.initialize().await?;

        let fs_path = self.to_fs_path(path).await;
        let mut all_files = Vec::new();

        self.list_files_recursive(fs_path, path.clone(), &mut all_files)
            .await?;

        debug!("Found {} files under {}", all_files.len(), path.uri());
        Ok(all_files)
    }

    /// Recursive helper for listing files.
    fn list_files_recursive<'a>(
        &'a self,
        fs_path: PathBuf,
        context_path: ContextPath,
        all_files: &'a mut Vec<ContextEntry>,
    ) -> Pin<Box<dyn Future<Output = WakeyResult<()>> + Send + 'a>> {
        Box::pin(async move {
            if !fs_path.exists() {
                return Ok(());
            }

            let mut read_dir = fs::read_dir(&fs_path).await?;

            while let Some(entry) = read_dir.next_entry().await? {
                let metadata = entry.metadata().await?;
                let name = entry.file_name().to_string_lossy().to_string();
                let child_fs_path = entry.path();

                let child_context_path = if context_path.path().is_empty() {
                    ContextPath::new(&format!("{}/{}", context_path.scope(), name))
                } else {
                    ContextPath::new(&format!(
                        "{}/{}/{}",
                        context_path.scope(),
                        context_path.path(),
                        name
                    ))
                };

                if metadata.is_dir() {
                    Box::pin(self.list_files_recursive(
                        child_fs_path,
                        child_context_path,
                        all_files,
                    ))
                    .await?;
                } else {
                    all_files.push(ContextEntry {
                        path: child_context_path,
                        is_dir: false,
                        size: metadata.len(),
                        mtime: metadata
                            .modified()
                            .map(|t| {
                                t.duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs() as i64
                            })
                            .unwrap_or(0),
                    });
                }
            }

            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_context_path_new() {
        let path = ContextPath::new("user/memories/preferences.md");
        assert_eq!(path.scope(), "user");
        assert_eq!(path.path(), "memories/preferences.md");
        assert_eq!(path.uri(), "wakey://user/memories/preferences.md");
    }

    #[test]
    fn test_context_path_from_uri() {
        let path = ContextPath::from_uri("wakey://agent/skills/test-skill/SKILL.md");
        assert_eq!(path.scope(), "agent");
        assert_eq!(path.path(), "skills/test-skill/SKILL.md");
    }

    #[test]
    fn test_context_path_parent() {
        let path = ContextPath::new("user/memories/preferences.md");
        let parent = path.parent().unwrap();
        assert_eq!(parent.scope(), "user");
        assert_eq!(parent.path(), "memories");
    }

    #[tokio::test]
    async fn test_context_fs_crud() {
        let temp_dir = tempdir().unwrap();
        let fs = ContextFs::new(temp_dir.path().to_path_buf());

        let path = ContextPath::new("user/memories/test.md");
        fs.write(&path, "test content").await.unwrap();

        let content = fs.read(&path).await.unwrap();
        assert_eq!(content, "test content");

        assert!(fs.exists(&path).await.unwrap());

        let dir_path = ContextPath::new("user/memories");
        let entries = fs.list(&dir_path).await.unwrap();
        assert_eq!(entries.len(), 1);

        assert!(fs.delete(&path).await.unwrap());
        assert!(!fs.exists(&path).await.unwrap());
    }

    #[tokio::test]
    async fn test_context_fs_initialize_creates_presets() {
        let temp_dir = tempdir().unwrap();
        let fs = ContextFs::new(temp_dir.path().to_path_buf());

        fs.initialize().await.unwrap();

        for preset in PRESET_DIRS {
            let preset_path = temp_dir.path().join(preset);
            assert!(preset_path.exists());
        }
    }
}
