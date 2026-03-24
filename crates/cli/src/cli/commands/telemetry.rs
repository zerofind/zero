//! Telemetry command handlers

use foundation::output::Outputter;

/// Subcommands for telemetry management
#[derive(clap::Subcommand)]
pub enum TelemetryCommands {
    /// Show current telemetry status (enabled/disabled)
    Status,

    /// Print exactly what would be sent (JSON payload)
    Show,

    /// Disable telemetry collection
    Off,

    /// Enable telemetry collection
    On,

    /// Reset install ID (generates a new anonymous identity)
    Reset,
}

/// Handle telemetry subcommands
#[allow(clippy::unnecessary_wraps)] // Returns Result for CLI command consistency
pub fn cmd_telemetry(out: &Outputter, cmd: &TelemetryCommands) -> anyhow::Result<()> {
    match cmd {
        TelemetryCommands::Status => {
            let enabled = telemetry::is_enabled();
            if enabled {
                out.success("Telemetry is enabled");
                out.info("Run `zero telemetry show` to see what is collected");
                out.info("Run `zero telemetry off` to disable");
            } else {
                out.info("Telemetry is disabled");
                out.info("Run `zero telemetry on` to enable");
            }
        }

        TelemetryCommands::Show => {
            if let Some(payload) = telemetry::show_payload() {
                out.println(&payload);
            } else {
                out.info("No telemetry data collected yet");
            }
        }

        TelemetryCommands::Off => {
            telemetry::set_enabled(false);
            out.success("Telemetry disabled");
        }

        TelemetryCommands::On => {
            telemetry::set_enabled(true);
            out.success("Telemetry enabled");
        }

        TelemetryCommands::Reset => {
            telemetry::reset_install_id();
            out.success("Install ID reset (new anonymous identity)");
        }
    }

    Ok(())
}
