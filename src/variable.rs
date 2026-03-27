use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

/// Resolved variables ready for interpolation.
pub type Variables = HashMap<String, String>;

/// Resolve variables with precedence: CLI -var > var-file > PKR_VAR_* env > defaults.
pub fn resolve(
    template_vars: &HashMap<String, Value>,
    cli_vars: &[(String, String)],
    var_files: &[&Path],
) -> anyhow::Result<Variables> {
    let mut resolved = Variables::new();

    // 1. Template defaults (lowest precedence)
    for (key, default) in template_vars {
        match default {
            Value::String(s) => {
                resolved.insert(key.clone(), s.clone());
            }
            Value::Null => {
                // Required variable — no default, must be supplied.
            }
            other => {
                resolved.insert(key.clone(), other.to_string());
            }
        }
    }

    // 2. PKR_VAR_* environment variables
    for key in template_vars.keys() {
        let env_key = format!("PKR_VAR_{key}");
        if let Ok(val) = std::env::var(&env_key) {
            resolved.insert(key.clone(), val);
        }
    }

    // 3. Variable files (in order, later files override earlier)
    for var_file in var_files {
        let content = std::fs::read_to_string(var_file)
            .map_err(|e| anyhow::anyhow!("failed to read var file {}: {e}", var_file.display()))?;
        let file_vars: HashMap<String, Value> = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("failed to parse var file {}: {e}", var_file.display()))?;
        for (key, val) in file_vars {
            match val {
                Value::String(s) => {
                    resolved.insert(key, s);
                }
                other => {
                    resolved.insert(key, other.to_string());
                }
            }
        }
    }

    // 4. CLI -var flags (highest precedence)
    for (key, val) in cli_vars {
        resolved.insert(key.clone(), val.clone());
    }

    // Check for required variables without values
    for (key, default) in template_vars {
        if *default == Value::Null && !resolved.contains_key(key) {
            anyhow::bail!("variable '{key}' is required but has no value");
        }
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), Value::String("default".into()));
        let resolved = resolve(&vars, &[], &[]).unwrap();
        assert_eq!(resolved.get("name").unwrap(), "default");
    }

    #[test]
    fn test_cli_overrides_default() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), Value::String("default".into()));
        let cli = vec![("name".into(), "override".into())];
        let resolved = resolve(&vars, &cli, &[]).unwrap();
        assert_eq!(resolved.get("name").unwrap(), "override");
    }

    #[test]
    fn test_required_variable_missing() {
        let mut vars = HashMap::new();
        vars.insert("secret".into(), Value::Null);
        let result = resolve(&vars, &[], &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_env_override() {
        let mut vars = HashMap::new();
        vars.insert("test_var".into(), Value::String("default".into()));
        unsafe { std::env::set_var("PKR_VAR_test_var", "from_env") };
        let resolved = resolve(&vars, &[], &[]).unwrap();
        assert_eq!(resolved.get("test_var").unwrap(), "from_env");
        unsafe { std::env::remove_var("PKR_VAR_test_var") };
    }

    #[test]
    fn test_required_variable_supplied_via_cli() {
        let mut vars = HashMap::new();
        vars.insert("secret".into(), Value::Null);
        let cli = vec![("secret".into(), "s3cr3t".into())];
        let resolved = resolve(&vars, &cli, &[]).unwrap();
        assert_eq!(resolved.get("secret").unwrap(), "s3cr3t");
    }

    #[test]
    fn test_empty_string_default() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), Value::String(String::new()));
        let resolved = resolve(&vars, &[], &[]).unwrap();
        assert_eq!(resolved.get("name").unwrap(), "");
    }

    #[test]
    fn test_non_string_value_coerced() {
        let mut vars = HashMap::new();
        vars.insert("count".into(), serde_json::json!(42));
        let resolved = resolve(&vars, &[], &[]).unwrap();
        assert_eq!(resolved.get("count").unwrap(), "42");
    }

    #[test]
    fn test_boolean_value_coerced() {
        let mut vars = HashMap::new();
        vars.insert("flag".into(), serde_json::json!(true));
        let resolved = resolve(&vars, &[], &[]).unwrap();
        assert_eq!(resolved.get("flag").unwrap(), "true");
    }

    #[test]
    fn test_cli_overrides_env() {
        let mut vars = HashMap::new();
        vars.insert("var_x".into(), Value::String("default".into()));
        unsafe { std::env::set_var("PKR_VAR_var_x", "from_env") };
        let cli = vec![("var_x".into(), "from_cli".into())];
        let resolved = resolve(&vars, &cli, &[]).unwrap();
        assert_eq!(resolved.get("var_x").unwrap(), "from_cli");
        unsafe { std::env::remove_var("PKR_VAR_var_x") };
    }

    #[test]
    fn test_var_file_loading() {
        let dir = std::env::temp_dir();
        let var_file = dir.join("igata_test_vars.json");
        std::fs::write(&var_file, r#"{"name": "from_file", "extra": "bonus"}"#).unwrap();
        let mut vars = HashMap::new();
        vars.insert("name".into(), Value::String("default".into()));
        let resolved = resolve(&vars, &[], &[var_file.as_path()]).unwrap();
        assert_eq!(resolved.get("name").unwrap(), "from_file");
        assert_eq!(resolved.get("extra").unwrap(), "bonus");
        std::fs::remove_file(&var_file).unwrap();
    }

    #[test]
    fn test_cli_overrides_var_file() {
        let dir = std::env::temp_dir();
        let var_file = dir.join("igata_test_vars2.json");
        std::fs::write(&var_file, r#"{"name": "from_file"}"#).unwrap();
        let mut vars = HashMap::new();
        vars.insert("name".into(), Value::String("default".into()));
        let cli = vec![("name".into(), "from_cli".into())];
        let resolved = resolve(&vars, &cli, &[var_file.as_path()]).unwrap();
        assert_eq!(resolved.get("name").unwrap(), "from_cli");
        std::fs::remove_file(&var_file).unwrap();
    }

    #[test]
    fn test_multiple_variables() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), Value::String("1".into()));
        vars.insert("b".into(), Value::String("2".into()));
        vars.insert("c".into(), Value::Null);
        let cli = vec![("c".into(), "3".into())];
        let resolved = resolve(&vars, &cli, &[]).unwrap();
        assert_eq!(resolved.get("a").unwrap(), "1");
        assert_eq!(resolved.get("b").unwrap(), "2");
        assert_eq!(resolved.get("c").unwrap(), "3");
    }

    #[test]
    fn test_empty_variables() {
        let vars = HashMap::new();
        let resolved = resolve(&vars, &[], &[]).unwrap();
        assert!(resolved.is_empty());
    }
}
