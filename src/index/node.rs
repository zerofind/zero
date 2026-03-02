//! File node representation for the search index

use serde::{Deserialize, Serialize};

/// Type of file system node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    File,
    Directory,
    Symlink,
}

/// A file node in the search index
///
/// This is the core data structure stored in the slab.
/// It contains just enough information for fast searching
/// and result display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    /// Filename only (e.g., "report.pdf")
    pub name: String,

    /// Full path from index root (e.g., "Documents/Work/report.pdf")
    pub path: String,

    /// Type of node
    pub node_type: NodeType,

    /// File size in bytes (0 for directories)
    pub size: u64,

    /// Last modification time (unix timestamp)
    pub mtime: u64,
}

impl FileNode {
    /// Create a new file node
    pub fn new(name: String, path: String, node_type: NodeType, size: u64, mtime: u64) -> Self {
        Self {
            name,
            path,
            node_type,
            size,
            mtime,
        }
    }

    /// Create a file node
    pub fn file(name: String, path: String, size: u64, mtime: u64) -> Self {
        Self::new(name, path, NodeType::File, size, mtime)
    }

    /// Create a directory node
    pub fn directory(name: String, path: String, mtime: u64) -> Self {
        Self::new(name, path, NodeType::Directory, 0, mtime)
    }

    /// Check if this is a file
    #[inline]
    pub fn is_file(&self) -> bool {
        self.node_type == NodeType::File
    }

    /// Check if this is a directory
    #[inline]
    pub fn is_directory(&self) -> bool {
        self.node_type == NodeType::Directory
    }

    /// Get the file extension (lowercase, without dot)
    pub fn extension(&self) -> Option<&str> {
        self.name.rsplit('.').next().filter(|ext| {
            // Make sure there was actually a dot (not just the filename)
            ext.len() < self.name.len()
        })
    }

    /// Get file type category as a numeric code
    /// 0=unknown, 1=folder, 2=image, 3=video, 4=audio, 5=document, 6=code, 7=archive, 8=config
    pub fn file_type_category(&self) -> u8 {
        if self.is_directory() {
            return 1; // folder
        }

        let ext = match self.extension() {
            Some(e) => e.to_lowercase(),
            None => return 0, // unknown
        };

        match ext.as_str() {
            // Images
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "ico" | "tiff" | "tif"
            | "heic" | "heif" | "raw" | "cr2" | "nef" | "arw" | "dng" | "psd" | "ai" | "eps" => 2,

            // Videos
            "mp4" | "mov" | "avi" | "mkv" | "wmv" | "flv" | "webm" | "m4v" | "mpeg" | "mpg"
            | "3gp" | "ogv" => 3,

            // Audio
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "wma" | "m4a" | "aiff" | "opus" | "mid"
            | "midi" => 4,

            // Documents
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp"
            | "rtf" | "txt" | "md" | "markdown" | "csv" | "pages" | "numbers" | "key" => 5,

            // Code
            "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "java" | "c" | "cpp" | "h" | "hpp"
            | "go" | "rb" | "php" | "swift" | "kt" | "scala" | "cs" | "vb" | "lua" | "pl"
            | "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" | "html" | "htm" | "css"
            | "scss" | "sass" | "less" | "vue" | "svelte" | "sql" | "r" | "m" | "mm" | "asm"
            | "s" | "zig" | "nim" | "d" | "ex" | "exs" | "erl" | "hrl" | "clj" | "cljs"
            | "cljc" | "elm" | "hs" | "ml" | "mli" | "fs" | "fsx" | "fsi" => 6,

            // Archives
            "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "tgz" | "tbz2" | "txz" | "lz"
            | "lzma" | "cab" | "iso" | "dmg" | "pkg" | "deb" | "rpm" => 7,

            // Config
            "json" | "yaml" | "yml" | "toml" | "xml" | "ini" | "conf" | "cfg" | "env"
            | "properties" | "plist" => 8,

            _ => 0, // unknown
        }
    }

    /// Get human-readable kind/type description
    pub fn kind_description(&self) -> String {
        if self.is_directory() {
            return "Folder".to_string();
        }

        let ext = match self.extension() {
            Some(e) => e.to_lowercase(),
            None => return "Document".to_string(),
        };

        match ext.as_str() {
            // Images
            "jpg" | "jpeg" => "JPEG Image".to_string(),
            "png" => "PNG Image".to_string(),
            "gif" => "GIF Image".to_string(),
            "bmp" => "BMP Image".to_string(),
            "svg" => "SVG Image".to_string(),
            "webp" => "WebP Image".to_string(),
            "ico" => "Icon".to_string(),
            "tiff" | "tif" => "TIFF Image".to_string(),
            "heic" | "heif" => "HEIC Image".to_string(),
            "raw" | "cr2" | "nef" | "arw" | "dng" => "RAW Image".to_string(),
            "psd" => "Photoshop Document".to_string(),
            "ai" => "Illustrator Document".to_string(),
            "eps" => "EPS Image".to_string(),

            // Videos
            "mp4" => "MPEG-4 Video".to_string(),
            "mov" => "QuickTime Movie".to_string(),
            "avi" => "AVI Video".to_string(),
            "mkv" => "Matroska Video".to_string(),
            "wmv" => "Windows Media Video".to_string(),
            "flv" => "Flash Video".to_string(),
            "webm" => "WebM Video".to_string(),
            "m4v" => "M4V Video".to_string(),
            "mpeg" | "mpg" => "MPEG Video".to_string(),
            "3gp" => "3GP Video".to_string(),
            "ogv" => "Ogg Video".to_string(),

            // Audio
            "mp3" => "MP3 Audio".to_string(),
            "wav" => "WAV Audio".to_string(),
            "flac" => "FLAC Audio".to_string(),
            "aac" => "AAC Audio".to_string(),
            "ogg" => "Ogg Audio".to_string(),
            "wma" => "Windows Media Audio".to_string(),
            "m4a" => "M4A Audio".to_string(),
            "aiff" => "AIFF Audio".to_string(),
            "opus" => "Opus Audio".to_string(),
            "mid" | "midi" => "MIDI Audio".to_string(),

            // Documents
            "pdf" => "PDF Document".to_string(),
            "doc" => "Word Document".to_string(),
            "docx" => "Word Document".to_string(),
            "xls" => "Excel Spreadsheet".to_string(),
            "xlsx" => "Excel Spreadsheet".to_string(),
            "ppt" => "PowerPoint Presentation".to_string(),
            "pptx" => "PowerPoint Presentation".to_string(),
            "odt" => "OpenDocument Text".to_string(),
            "ods" => "OpenDocument Spreadsheet".to_string(),
            "odp" => "OpenDocument Presentation".to_string(),
            "rtf" => "Rich Text Document".to_string(),
            "txt" => "Plain Text".to_string(),
            "md" | "markdown" => "Markdown Document".to_string(),
            "csv" => "CSV Document".to_string(),
            "pages" => "Pages Document".to_string(),
            "numbers" => "Numbers Spreadsheet".to_string(),
            "key" => "Keynote Presentation".to_string(),

            // Code
            "rs" => "Rust Source".to_string(),
            "py" => "Python Source".to_string(),
            "js" => "JavaScript Source".to_string(),
            "ts" => "TypeScript Source".to_string(),
            "jsx" => "JSX Source".to_string(),
            "tsx" => "TSX Source".to_string(),
            "java" => "Java Source".to_string(),
            "c" => "C Source".to_string(),
            "cpp" => "C++ Source".to_string(),
            "h" => "C Header".to_string(),
            "hpp" => "C++ Header".to_string(),
            "go" => "Go Source".to_string(),
            "rb" => "Ruby Source".to_string(),
            "php" => "PHP Source".to_string(),
            "swift" => "Swift Source".to_string(),
            "kt" => "Kotlin Source".to_string(),
            "scala" => "Scala Source".to_string(),
            "cs" => "C# Source".to_string(),
            "html" | "htm" => "HTML Document".to_string(),
            "css" => "CSS Stylesheet".to_string(),
            "scss" | "sass" => "Sass Stylesheet".to_string(),
            "less" => "LESS Stylesheet".to_string(),
            "vue" => "Vue Component".to_string(),
            "svelte" => "Svelte Component".to_string(),
            "sql" => "SQL Script".to_string(),
            "sh" | "bash" => "Shell Script".to_string(),
            "zsh" => "Zsh Script".to_string(),
            "fish" => "Fish Script".to_string(),
            "ps1" => "PowerShell Script".to_string(),
            "bat" | "cmd" => "Batch Script".to_string(),

            // Archives
            "zip" => "ZIP Archive".to_string(),
            "tar" => "TAR Archive".to_string(),
            "gz" => "Gzip Archive".to_string(),
            "bz2" => "Bzip2 Archive".to_string(),
            "xz" => "XZ Archive".to_string(),
            "7z" => "7-Zip Archive".to_string(),
            "rar" => "RAR Archive".to_string(),
            "tgz" | "tbz2" | "txz" => "Compressed Archive".to_string(),
            "iso" => "Disk Image".to_string(),
            "dmg" => "macOS Disk Image".to_string(),
            "pkg" => "Installer Package".to_string(),
            "deb" => "Debian Package".to_string(),
            "rpm" => "RPM Package".to_string(),

            // Config
            "json" => "JSON Document".to_string(),
            "yaml" | "yml" => "YAML Document".to_string(),
            "toml" => "TOML Document".to_string(),
            "xml" => "XML Document".to_string(),
            "ini" => "INI Configuration".to_string(),
            "conf" | "cfg" => "Configuration File".to_string(),
            "env" => "Environment File".to_string(),
            "plist" => "Property List".to_string(),

            _ => format!("{} File", ext.to_uppercase()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_node_creation() {
        let node = FileNode::file(
            "report.pdf".to_string(),
            "Documents/report.pdf".to_string(),
            1024,
            1700000000,
        );

        assert_eq!(node.name, "report.pdf");
        assert_eq!(node.path, "Documents/report.pdf");
        assert!(node.is_file());
        assert!(!node.is_directory());
        assert_eq!(node.size, 1024);
    }

    #[test]
    fn test_directory_node() {
        let node =
            FileNode::directory("Documents".to_string(), "Documents".to_string(), 1700000000);

        assert!(node.is_directory());
        assert!(!node.is_file());
        assert_eq!(node.size, 0);
    }

    #[test]
    fn test_extension() {
        let pdf = FileNode::file("report.pdf".into(), "report.pdf".into(), 0, 0);
        assert_eq!(pdf.extension(), Some("pdf"));

        let tar_gz = FileNode::file("archive.tar.gz".into(), "archive.tar.gz".into(), 0, 0);
        assert_eq!(tar_gz.extension(), Some("gz"));

        let no_ext = FileNode::file("README".into(), "README".into(), 0, 0);
        assert_eq!(no_ext.extension(), None);

        let hidden = FileNode::file(".gitignore".into(), ".gitignore".into(), 0, 0);
        assert_eq!(hidden.extension(), Some("gitignore"));
    }
}
