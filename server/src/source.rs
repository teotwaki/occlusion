//! Data source abstraction for loading store data from files or URLs.

use std::path::PathBuf;
use std::time::SystemTime;

/// Represents a data source for loading store data.
#[derive(Debug, Clone)]
pub enum DataSource {
    /// Local file path
    File(PathBuf),
    /// HTTP(S) URL
    Url(String),
}

impl DataSource {
    /// Parse a string into a DataSource.
    ///
    /// Strings starting with "http://" or "https://" are treated as URLs,
    /// everything else is treated as a file path.
    pub fn parse(s: &str) -> Self {
        if s.starts_with("http://") || s.starts_with("https://") {
            return DataSource::Url(s.to_string());
        }
        DataSource::File(PathBuf::from(s))
    }

    /// Returns true if this is a URL source.
    pub fn is_url(&self) -> bool {
        matches!(self, DataSource::Url(_))
    }

    /// Returns true if this is a file source.
    pub fn is_file(&self) -> bool {
        matches!(self, DataSource::File(_))
    }
}

impl std::fmt::Display for DataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSource::File(path) => write!(f, "{}", path.display()),
            DataSource::Url(url) => write!(f, "{}", url),
        }
    }
}

/// Metadata about a data source used for conditional reloading.
///
/// For files, this tracks the modification time.
/// For URLs, this tracks ETag and Last-Modified headers.
#[derive(Debug, Clone, Default)]
pub struct SourceMetadata {
    /// File modification time (for file sources)
    pub mtime: Option<SystemTime>,
    /// ETag header value (for URL sources)
    pub etag: Option<String>,
    /// Last-Modified header value (for URL sources)
    pub last_modified: Option<String>,
}

impl SourceMetadata {
    /// Create empty metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create metadata from a file's modification time.
    pub fn from_file(path: &std::path::Path) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        let mtime = metadata.modified()?;
        Ok(Self {
            mtime: Some(mtime),
            etag: None,
            last_modified: None,
        })
    }

    /// Check if the source has changed compared to this metadata.
    ///
    /// For files, compares modification time.
    /// For URLs, should be called with metadata from a HEAD request.
    pub fn has_changed(&self, other: &SourceMetadata) -> bool {
        if self.mtime.is_none() && self.etag.is_none() && self.last_modified.is_none() {
            return true;
        }

        if let (Some(old_mtime), Some(new_mtime)) = (self.mtime, other.mtime) {
            return new_mtime > old_mtime;
        }

        if let (Some(old_etag), Some(new_etag)) = (&self.etag, &other.etag) {
            return old_etag != new_etag;
        }

        if let (Some(old_lm), Some(new_lm)) = (&self.last_modified, &other.last_modified) {
            return old_lm != new_lm;
        }

        true
    }
}
