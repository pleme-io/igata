use colored::Colorize;

use crate::template::Template;

/// Print a human-readable summary of a template.
pub fn inspect(template: &Template) {
    if let Some(desc) = &template.description {
        println!("{} {desc}", "Description:".bold());
        println!();
    }

    if let Some(ver) = &template.min_version {
        println!("{} {ver}", "Min version:".bold());
        println!();
    }

    // Variables
    if !template.variables.is_empty() {
        println!("{}", "Variables:".bold());
        for (name, default) in &template.variables {
            let default_str = match default {
                serde_json::Value::Null => "(required)".red().to_string(),
                serde_json::Value::String(s) => format!("\"{}\"", s.green()),
                other => other.to_string(),
            };
            println!("  {name:<30} = {default_str}");
        }
        println!();
    }

    // Builders
    println!("{} ({})", "Builders:".bold(), template.builders.len());
    for builder in &template.builders {
        let name = builder.effective_name();
        let btype = &builder.builder_type;
        if Some(name) == Some(btype.as_str()) {
            println!("  {name}");
        } else {
            println!("  {} (type: {})", name.cyan(), btype);
        }
    }
    println!();

    // Provisioners
    if !template.provisioners.is_empty() {
        println!(
            "{} ({})",
            "Provisioners:".bold(),
            template.provisioners.len()
        );
        for prov in &template.provisioners {
            let mut suffix = String::new();
            if !prov.only.is_empty() {
                suffix = format!(" [only: {}]", prov.only.join(", "));
            } else if !prov.except.is_empty() {
                suffix = format!(" [except: {}]", prov.except.join(", "));
            }
            println!("  {}{suffix}", prov.provisioner_type);
        }
        println!();
    }

    // Post-processors
    if !template.post_processors.is_empty() {
        println!(
            "{} ({})",
            "Post-processors:".bold(),
            template.post_processors.len()
        );
        for (i, entry) in template.post_processors.iter().enumerate() {
            let pipeline = entry.as_pipeline();
            if pipeline.len() == 1 {
                println!("  {}", pipeline[0].pp_type);
            } else {
                let names: Vec<&str> = pipeline.iter().map(|p| p.pp_type.as_str()).collect();
                println!("  pipeline[{i}]: {}", names.join(" -> "));
            }
        }
        println!();
    }
}
