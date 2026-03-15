use clap::{Parser, Subcommand};
use igata::{Context, Engine, Manifest, Syntax};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "igata", about = "鋳型 — template engine for Nix activation-time rendering")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render templates from a JSON manifest (Nix integration).
    Render {
        /// Path to the manifest JSON file.
        #[arg(long)]
        manifest: PathBuf,
    },

    /// Render a single template file.
    File {
        /// Path to the template file.
        #[arg(long)]
        template: PathBuf,

        /// Output path (stdout if omitted).
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Variable: NAME=VALUE (literal) or NAME=@PATH (from file) or NAME=$ENV (from env).
        #[arg(long = "var", short = 'v')]
        vars: Vec<String>,

        /// File permission mode (octal, default 0600).
        #[arg(long, default_value = "0600")]
        mode: String,
    },

    /// Validate a template without rendering.
    Check {
        /// Path to the template file.
        #[arg(long)]
        template: PathBuf,

        /// Expected variable names (comma-separated).
        #[arg(long)]
        vars: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Render { manifest } => {
            let manifest = Manifest::load(&manifest)?;
            let engine = Engine::with_syntax(manifest.syntax.clone())?;
            let report = engine.render_manifest(&manifest)?;
            eprintln!(
                "[igata] rendered {} template(s)",
                report.rendered.len()
            );
        }

        Command::File {
            template,
            output,
            vars,
            mode,
        } => {
            let ctx = parse_vars(&vars)?;
            let engine = Engine::new();
            let rendered = engine.render_file(&template, &ctx)?;

            if let Some(out) = output {
                let mode_int = u32::from_str_radix(&mode, 8).unwrap_or(0o600);
                engine.render_to_file(&template, &out, &ctx, mode_int)?;
                eprintln!("[igata] rendered → {}", out.display());
            } else {
                print!("{rendered}");
            }
        }

        Command::Check { template, vars } => {
            let content = std::fs::read_to_string(&template)?;
            let syntax = Syntax::default();
            let mut env = minijinja::Environment::new();
            env.set_syntax(syntax.to_config()?);
            // Parse without rendering — validates syntax only.
            let _ = env.template_from_str(&content)?;
            eprintln!("[igata] ✓ template valid: {}", template.display());

            if let Some(expected) = vars {
                let names: Vec<&str> = expected.split(',').map(str::trim).collect();
                eprintln!("[igata]   expected variables: {}", names.join(", "));
            }
        }
    }

    Ok(())
}

fn parse_vars(vars: &[String]) -> anyhow::Result<Context> {
    let mut builder = Context::builder();

    for var in vars {
        let (name, value) = var
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid var format: {var} (expected NAME=VALUE, NAME=@PATH, or NAME=$ENV)"))?;

        if let Some(path) = value.strip_prefix('@') {
            builder = builder.file(name, path);
        } else if let Some(env_name) = value.strip_prefix('$') {
            builder = builder.env(name, env_name);
        } else {
            builder = builder.literal(name, value);
        }
    }

    Ok(builder.build())
}
