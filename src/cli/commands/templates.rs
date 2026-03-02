//! Templates command handler

use anyhow::Result;
use zero::output::Outputter;
use zero::templates::{all_templates, get_template};

pub fn cmd_templates(out: &Outputter, show: Option<String>, resolve: bool) -> Result<()> {
    if let Some(template_id) = show {
        // Show specific template
        let template = get_template(&template_id)
            .ok_or_else(|| anyhow::anyhow!("Template not found: {}", template_id))?;

        if resolve {
            let resolved = template.resolve();
            out.header(&format!("Template: {}", resolved.template.name));
            out.newline();
            out.info(resolved.template.description);
            out.newline();

            // Show sources with existence status
            out.info("Sources:");
            for src in &resolved.sources {
                let status = if src.exists {
                    "✓"
                } else if src.source.optional {
                    "○"
                } else {
                    "✗"
                };
                let optional_marker = if src.source.optional {
                    " (optional)"
                } else {
                    ""
                };
                out.indented(&format!(
                    "{} ~/{} - {}{}",
                    status, src.source.path, src.source.description, optional_marker
                ));
            }

            // Show detected cloud folders
            if !resolved.detected_cloud_folders.is_empty() {
                out.newline();
                out.info("Auto-excluded cloud folders:");
                for cloud in &resolved.detected_cloud_folders {
                    out.indented(&format!("• {}", cloud.display()));
                }
            }

            // Show excludes
            if !resolved.excludes.is_empty() {
                out.newline();
                out.info(&format!("Excludes ({}):", resolved.excludes.len()));
                // Show first 10 excludes
                for exclude in resolved.excludes.iter().take(10) {
                    out.indented(&format!("• {}", exclude));
                }
                if resolved.excludes.len() > 10 {
                    out.indented(&format!("... and {} more", resolved.excludes.len() - 10));
                }
            }

            // Show validation
            out.newline();
            let missing = resolved.missing_required_sources();
            if missing.is_empty() {
                out.success("Template is valid - all required sources exist");
            } else {
                out.error(&format!("Missing {} required source(s):", missing.len()));
                for src in missing {
                    out.indented(&format!("✗ ~/{}", src.source.path));
                }
            }

            // Show existing sources count
            let existing = resolved.existing_sources();
            out.newline();
            out.kv("Would backup", format!("{} folder(s)", existing.len()));
        } else {
            // Show template without resolution
            out.header(&format!("Template: {}", template.name));
            out.newline();
            out.info(template.description);
            out.newline();

            out.info("Sources:");
            for src in template.sources {
                let marker = if src.optional { "○" } else { "●" };
                out.indented(&format!("{} ~/{} - {}", marker, src.path, src.description));
            }

            if !template.excludes.is_empty() {
                out.newline();
                out.info(&format!("Excludes ({}):", template.excludes.len()));
                for exclude in template.excludes.iter().take(10) {
                    out.indented(&format!("• {}", exclude));
                }
                if template.excludes.len() > 10 {
                    out.indented(&format!("... and {} more", template.excludes.len() - 10));
                }
            }

            out.newline();
            out.info("Run with --resolve to see which paths exist on your system");
        }
    } else {
        // List all templates
        out.header("Available Templates");
        out.newline();

        for template in all_templates() {
            out.info(&format!("{} ({})", template.name, template.id));
            out.indented(template.description);

            // Count sources
            let required = template.sources.iter().filter(|s| !s.optional).count();
            let optional = template.sources.iter().filter(|s| s.optional).count();
            out.indented(&format!(
                "{} required + {} optional source(s), {} exclude pattern(s)",
                required,
                optional,
                template.excludes.len()
            ));
            out.newline();
        }

        out.info("Use --show <id> to see template details");
        out.info("Use --show <id> --resolve to check which paths exist");
    }

    Ok(())
}
