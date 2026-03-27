mod build;
mod builder;
mod communicator;
mod config;
mod display;
mod error;
mod inspect;
mod interpolation;
mod post_processor;
mod provisioner;
mod template;
mod traits;
mod validate;
mod variable;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "igata",
    version,
    about = "igata — Nix-first machine image builder (Packer-compatible)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build images from a Packer JSON template
    Build {
        /// Template file
        template: PathBuf,

        /// Set a variable: key=value
        #[arg(long = "var", short = 'v')]
        vars: Vec<String>,

        /// Variable file (JSON)
        #[arg(long = "var-file")]
        var_files: Vec<PathBuf>,

        /// Build only the specified builders
        #[arg(long)]
        only: Vec<String>,

        /// Skip the specified builders
        #[arg(long)]
        except: Vec<String>,

        /// Force a build even if artifacts exist
        #[arg(long)]
        force: bool,

        /// On-error behavior: cleanup, abort, or ask
        #[arg(long, default_value = "cleanup")]
        on_error: String,

        /// Number of parallel builds
        #[arg(long, default_value = "1")]
        parallel_builds: usize,

        /// Disable color output
        #[arg(long)]
        no_color: bool,

        /// Machine-readable output
        #[arg(long)]
        machine_readable: bool,

        /// Prefix output with timestamps
        #[arg(long)]
        timestamp_ui: bool,
    },

    /// Validate a Packer JSON template
    Validate {
        /// Template file
        template: PathBuf,

        /// Set a variable: key=value
        #[arg(long = "var", short = 'v')]
        vars: Vec<String>,

        /// Variable file (JSON)
        #[arg(long = "var-file")]
        var_files: Vec<PathBuf>,
    },

    /// Display template summary
    Inspect {
        /// Template file
        template: PathBuf,
    },

    /// Show version
    Version,
}

fn parse_cli_vars(vars: &[String]) -> Vec<(String, String)> {
    vars.iter()
        .filter_map(|v| {
            v.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            template: template_path,
            vars,
            var_files,
            only,
            except,
            force,
            on_error,
            parallel_builds,
            no_color,
            machine_readable,
            timestamp_ui,
        } => {
            if no_color {
                colored::control::set_override(false);
            }

            let tmpl = template::parse_file(&template_path)
                .with_context(|| format!("parsing {}", template_path.display()))?;

            // Validate first
            let validation = validate::validate(&tmpl);
            for err in &validation.errors {
                display::print_validation_error(err);
            }
            for warn in &validation.warnings {
                display::print_validation_warning(warn);
            }
            if !validation.is_ok() {
                anyhow::bail!("template validation failed");
            }

            // Resolve variables
            let cli_vars = parse_cli_vars(&vars);
            let var_file_refs: Vec<&std::path::Path> =
                var_files.iter().map(|p| p.as_path()).collect();
            let variables =
                variable::resolve(&tmpl.variables, &cli_vars, &var_file_refs)?;

            // Build registry
            let mut registry = traits::Registry::new();
            builder::register_all(&mut registry);
            provisioner::register_all(&mut registry);
            post_processor::register_all(&mut registry);

            // Load config
            let cfg = config::load();

            let template_dir = template_path
                .parent()
                .map(|p| p.to_string_lossy().to_string());

            let opts = build::BuildOptions {
                only,
                except,
                on_error: build::OnError::parse(&on_error)?,
                parallel_builds,
                force,
                machine_readable,
                timestamp_ui,
                no_color,
                template_dir,
            };

            let result = build::run(&tmpl, &variables, &registry, &cfg, &opts).await;

            if !result.errors.is_empty() {
                let count = result.errors.len();
                anyhow::bail!("{count} build(s) failed");
            }

            println!(
                "\n==> Builds finished. {} artifact(s) produced.",
                result.artifacts.len()
            );
        }

        Commands::Validate {
            template: template_path,
            vars,
            var_files,
        } => {
            let tmpl = template::parse_file(&template_path)
                .with_context(|| format!("parsing {}", template_path.display()))?;

            let validation = validate::validate(&tmpl);

            for err in &validation.errors {
                display::print_validation_error(err);
            }
            for warn in &validation.warnings {
                display::print_validation_warning(warn);
            }

            // Also validate variables resolve
            let cli_vars = parse_cli_vars(&vars);
            let var_file_refs: Vec<&std::path::Path> =
                var_files.iter().map(|p| p.as_path()).collect();
            if let Err(e) = variable::resolve(&tmpl.variables, &cli_vars, &var_file_refs)
            {
                display::print_validation_error(&e.to_string());
                anyhow::bail!("template validation failed");
            }

            if validation.is_ok() {
                println!("Template validated successfully.");
            } else {
                anyhow::bail!("template validation failed");
            }
        }

        Commands::Inspect {
            template: template_path,
        } => {
            let tmpl = template::parse_file(&template_path)
                .with_context(|| format!("parsing {}", template_path.display()))?;
            inspect::inspect(&tmpl);
        }

        Commands::Version => {
            println!("igata {}", env!("CARGO_PKG_VERSION"));
        }
    }

    Ok(())
}
