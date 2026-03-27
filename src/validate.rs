use crate::template::Template;

/// Validation result with accumulated errors.
#[derive(Debug, Default)]
pub struct ValidationResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Validate a parsed template for correctness.
pub fn validate(template: &Template) -> ValidationResult {
    let mut result = ValidationResult::default();

    // Check sensitive-variables reference defined variables
    for sv in &template.sensitive_variables {
        if !template.variables.contains_key(sv) {
            result.errors.push(format!(
                "sensitive-variables: '{sv}' is not defined in variables"
            ));
        }
    }

    // Must have at least one builder
    if template.builders.is_empty() {
        result
            .errors
            .push("template must have at least one builder".into());
    }

    // Check builder types
    let known_builders = ["null", "docker", "qemu", "amazon-ebs"];
    for (i, builder) in template.builders.iter().enumerate() {
        if builder.builder_type.is_empty() {
            result
                .errors
                .push(format!("builder[{i}]: 'type' is required"));
        } else if !known_builders.contains(&builder.builder_type.as_str()) {
            result.warnings.push(format!(
                "builder[{i}]: unknown type '{}' (known: {})",
                builder.builder_type,
                known_builders.join(", ")
            ));
        }
    }

    // Check for duplicate builder names
    let mut seen_names = std::collections::HashSet::new();
    for builder in &template.builders {
        let name = builder.effective_name();
        if !seen_names.insert(name.to_string()) {
            result
                .errors
                .push(format!("duplicate builder name: '{name}'"));
        }
    }

    // Check provisioner types
    let known_provisioners = ["shell", "file", "shell-local", "breakpoint"];
    for (i, prov) in template.provisioners.iter().enumerate() {
        if prov.provisioner_type.is_empty() {
            result
                .errors
                .push(format!("provisioner[{i}]: 'type' is required"));
        } else if !known_provisioners.contains(&prov.provisioner_type.as_str()) {
            result.warnings.push(format!(
                "provisioner[{i}]: unknown type '{}'",
                prov.provisioner_type
            ));
        }
    }

    // Check post-processor types
    let known_pps = ["manifest", "checksum", "shell-local", "compress"];
    for (i, entry) in template.post_processors.iter().enumerate() {
        for pp in entry.as_pipeline() {
            if pp.pp_type.is_empty() {
                result
                    .errors
                    .push(format!("post-processor[{i}]: 'type' is required"));
            } else if !known_pps.contains(&pp.pp_type.as_str()) {
                result.warnings.push(format!(
                    "post-processor[{i}]: unknown type '{}'",
                    pp.pp_type
                ));
            }
        }
    }

    // Check only/except references point to valid builder names
    let builder_names: Vec<String> = template
        .builders
        .iter()
        .map(|b| b.effective_name().to_string())
        .collect();

    for (i, prov) in template.provisioners.iter().enumerate() {
        for name in &prov.only {
            if !builder_names.contains(name) {
                result.errors.push(format!(
                    "provisioner[{i}]: 'only' references unknown builder '{name}'"
                ));
            }
        }
        for name in &prov.except {
            if !builder_names.contains(name) {
                result.errors.push(format!(
                    "provisioner[{i}]: 'except' references unknown builder '{name}'"
                ));
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template;

    #[test]
    fn test_valid_template() {
        let tmpl = template::parse_json(r#"{"builders": [{"type": "null"}]}"#).unwrap();
        let result = validate(&tmpl);
        assert!(result.is_ok());
    }

    #[test]
    fn test_no_builders() {
        let tmpl = template::parse_json(r#"{"builders": []}"#).unwrap();
        let result = validate(&tmpl);
        assert!(!result.is_ok());
    }

    #[test]
    fn test_duplicate_builder_names() {
        let tmpl = template::parse_json(
            r#"{"builders": [{"type": "null", "name": "a"}, {"type": "null", "name": "a"}]}"#,
        )
        .unwrap();
        let result = validate(&tmpl);
        assert!(!result.is_ok());
    }

    #[test]
    fn test_invalid_only_reference() {
        let tmpl = template::parse_json(
            r#"{"builders": [{"type": "null"}], "provisioners": [{"type": "shell", "only": ["nonexistent"]}]}"#,
        )
        .unwrap();
        let result = validate(&tmpl);
        assert!(!result.is_ok());
    }

    #[test]
    fn test_unknown_builder_type_is_warning_not_error() {
        let tmpl =
            template::parse_json(r#"{"builders": [{"type": "unknown-builder"}]}"#).unwrap();
        let result = validate(&tmpl);
        assert!(result.is_ok()); // warnings don't fail validation
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_unknown_provisioner_type_is_warning() {
        let tmpl = template::parse_json(
            r#"{"builders": [{"type": "null"}], "provisioners": [{"type": "unknown-prov"}]}"#,
        )
        .unwrap();
        let result = validate(&tmpl);
        assert!(result.is_ok());
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_unknown_post_processor_type_is_warning() {
        let tmpl = template::parse_json(
            r#"{"builders": [{"type": "null"}], "post-processors": [{"type": "unknown-pp"}]}"#,
        )
        .unwrap();
        let result = validate(&tmpl);
        assert!(result.is_ok());
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_except_references_unknown_builder() {
        let tmpl = template::parse_json(
            r#"{"builders": [{"type": "null"}], "provisioners": [{"type": "shell", "except": ["nonexistent"]}]}"#,
        )
        .unwrap();
        let result = validate(&tmpl);
        assert!(!result.is_ok());
    }

    #[test]
    fn test_multiple_known_builders() {
        let tmpl = template::parse_json(
            r#"{"builders": [{"type": "null", "name": "a"}, {"type": "docker", "name": "b"}]}"#,
        )
        .unwrap();
        let result = validate(&tmpl);
        assert!(result.is_ok());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_post_processor_pipeline_validation() {
        let tmpl = template::parse_json(
            r#"{"builders": [{"type": "null"}], "post-processors": [[{"type": "checksum"}, {"type": "manifest"}]]}"#,
        )
        .unwrap();
        let result = validate(&tmpl);
        assert!(result.is_ok());
    }

    #[test]
    fn test_no_provisioners_is_valid() {
        let tmpl = template::parse_json(r#"{"builders": [{"type": "null"}]}"#).unwrap();
        let result = validate(&tmpl);
        assert!(result.is_ok());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_valid_only_reference() {
        let tmpl = template::parse_json(
            r#"{"builders": [{"type": "null"}], "provisioners": [{"type": "shell", "only": ["null"]}]}"#,
        )
        .unwrap();
        let result = validate(&tmpl);
        assert!(result.is_ok());
    }

    #[test]
    fn test_sensitive_variables_reference_valid() {
        let tmpl = template::parse_json(r#"{
            "sensitive-variables": ["secret"],
            "variables": {"secret": null, "name": "public"},
            "builders": [{"type": "null"}]
        }"#)
        .unwrap();
        let result = validate(&tmpl);
        assert!(result.is_ok());
    }

    #[test]
    fn test_sensitive_variables_reference_invalid() {
        let tmpl = template::parse_json(r#"{
            "sensitive-variables": ["nonexistent"],
            "variables": {"name": "public"},
            "builders": [{"type": "null"}]
        }"#)
        .unwrap();
        let result = validate(&tmpl);
        assert!(!result.is_ok());
        assert!(result.errors[0].contains("nonexistent"));
    }
}
