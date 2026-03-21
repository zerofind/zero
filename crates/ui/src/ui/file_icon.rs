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
            extension: extension.map(|e| e.to_string()),
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
            Some("rs") | Some("swift") => t.yellow,
            Some("py") => t.cyan,
            Some("js") | Some("ts") => t.yellow_light,
            Some("go") => t.cyan_light,
            Some("c") | Some("cpp") | Some("h") => t.blue,
            Some("java") | Some("rb") => t.red,
            Some("json") | Some("yaml") | Some("yml") | Some("toml") | Some("xml")
            | Some("csv") => t.muted_foreground,
            Some("jpg") | Some("jpeg") | Some("png") | Some("gif") | Some("svg") | Some("webp")
            | Some("bmp") | Some("ico") | Some("heic") | Some("tiff") | Some("tif") => t.magenta,
            Some("mp4") | Some("mov") | Some("avi") | Some("mkv") | Some("webm") | Some("m4v")
            | Some("wmv") | Some("flv") | Some("3gp") => t.red,
            Some("mp3") | Some("wav") | Some("flac") | Some("aac") | Some("ogg") | Some("m4a")
            | Some("wma") | Some("aiff") | Some("alac") => t.green,
            Some("pdf") => t.red_light,
            Some("md") | Some("txt") | Some("rtf") | Some("doc") | Some("docx") => {
                t.muted_foreground
            }
            Some("zip") | Some("tar") | Some("gz") | Some("bz2") | Some("xz") | Some("7z")
            | Some("rar") | Some("dmg") | Some("iso") => t.yellow,
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
