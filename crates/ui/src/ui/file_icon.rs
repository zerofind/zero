use gpui::*;
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _};

use crate::theme::ICON_XS;

/// Maps file extension to an appropriate icon and color.
#[derive(IntoElement)]
pub struct FileIcon {
    extension: Option<String>,
    is_dir: bool,
}

impl FileIcon {
    pub fn new(extension: Option<&str>, is_dir: bool) -> Self {
        Self {
            extension: extension.map(std::string::ToString::to_string),
            is_dir,
        }
    }

    /// Lowercase extension for case-insensitive matching.
    fn ext_lower(&self) -> Option<String> {
        self.extension.as_ref().map(|e| e.to_ascii_lowercase())
    }

    fn icon_name(&self) -> IconName {
        if self.is_dir {
            IconName::Folder
        } else {
            IconName::File
        }
    }

    /// Resolve icon color from the active theme's base palette.
    fn icon_color(&self, cx: &App) -> Hsla {
        let t = cx.theme();
        if self.is_dir {
            return t.blue;
        }
        match self.ext_lower().as_deref() {
            Some("rs" | "swift") => t.yellow,
            Some("py") => t.cyan,
            Some("js" | "ts") => t.yellow_light,
            Some("go") => t.cyan_light,
            Some("c" | "cpp" | "h") => t.blue,
            Some("java" | "rb") => t.red,
            Some("json" | "yaml" | "yml" | "toml" | "xml" | "csv") => t.muted_foreground,
            Some(
                "jpg" | "jpeg" | "png" | "gif" | "svg" | "webp" | "bmp" | "ico" | "heic" | "tiff"
                | "tif",
            ) => t.magenta,
            Some("mp4" | "mov" | "avi" | "mkv" | "webm" | "m4v" | "wmv" | "flv" | "3gp") => t.red,
            Some("mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "aiff" | "alac") => {
                t.green
            }
            Some("pdf") => t.red_light,
            Some("md" | "txt" | "rtf" | "doc" | "docx") => t.muted_foreground,
            Some("zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "dmg" | "iso") => t.yellow,
            _ => t.muted_foreground,
        }
    }
}

impl RenderOnce for FileIcon {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        Icon::new(self.icon_name())
            .with_size(ICON_XS)
            .text_color(self.icon_color(cx))
    }
}
