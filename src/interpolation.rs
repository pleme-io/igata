use gtmpl::{Context, FuncError, Template};
use gtmpl_value::Value as GtmplValue;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::variable::Variables;

// ---------------------------------------------------------------------------
// InterpolationContext — all data available during template evaluation
// ---------------------------------------------------------------------------

/// Build context available during interpolation.
///
/// This is the complete context that feeds every `{{...}}` expression.
/// The Go template engine (gtmpl) evaluates all expressions against this context.
/// Provisioners and post-processors extend it via `dot_context` for
/// fields like `{{.Vars}}`, `{{.Path}}`, `{{.Image}}`.
pub struct InterpolationContext<'a> {
    /// User-defined variables (from template `variables` + CLI overrides).
    pub variables: &'a Variables,
    /// Current builder name.
    pub build_name: &'a str,
    /// Current builder type (e.g., "docker", "qemu").
    pub build_type: &'a str,
    /// Directory containing the template file (for `{{template_dir}}`).
    pub template_dir: Option<&'a str>,
    /// Dot-context variables: `{{.Vars}}`, `{{.Path}}`, `{{.Name}}`, etc.
    pub dot_context: HashMap<String, String>,
}

impl<'a> InterpolationContext<'a> {
    pub fn new(
        variables: &'a Variables,
        build_name: &'a str,
        build_type: &'a str,
    ) -> Self {
        Self {
            variables,
            build_name,
            build_type,
            template_dir: None,
            dot_context: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Thread-local context for gtmpl function pointers
//
// gtmpl's Func type is `fn(&[Value]) -> Result<Value, FuncError>` (a plain
// function pointer, not a closure). We use thread-local storage to make the
// interpolation context available to registered functions.
// ---------------------------------------------------------------------------

/// Snapshot of interpolation context for thread-local access.
#[derive(Clone, Default)]
struct TlsContext {
    variables: Variables,
    build_name: String,
    build_type: String,
    template_dir: String,
}

thread_local! {
    static TLS_CTX: RefCell<TlsContext> = RefCell::new(TlsContext::default());
}

fn set_tls_ctx(ctx: &InterpolationContext<'_>) {
    TLS_CTX.with(|tls| {
        *tls.borrow_mut() = TlsContext {
            variables: ctx.variables.clone(),
            build_name: ctx.build_name.to_string(),
            build_type: ctx.build_type.to_string(),
            template_dir: ctx.template_dir.unwrap_or(".").to_string(),
        };
    });
}

fn with_tls_ctx<F, R>(f: F) -> R
where
    F: FnOnce(&TlsContext) -> R,
{
    TLS_CTX.with(|tls| f(&tls.borrow()))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Interpolate all `{{...}}` patterns in a string using the Go template engine.
///
/// Uses gtmpl (Go `text/template` implementation in Rust) for full Packer
/// compatibility: pipes, conditionals, range, dot access, and all Packer functions.
pub fn interpolate(input: &str, ctx: &InterpolationContext<'_>) -> anyhow::Result<String> {
    if !input.contains("{{") {
        return Ok(input.to_string());
    }

    // Set thread-local context for function pointers
    set_tls_ctx(ctx);

    let mut tmpl = Template::default();
    register_packer_funcs(&mut tmpl);

    tmpl.parse(input)
        .map_err(|e| anyhow::anyhow!("template parse error: {e}"))?;

    let data = Context::from(build_dot_data(ctx));

    tmpl.render(&data)
        .map_err(|e| anyhow::anyhow!("template render error: {e}"))
}

/// Recursively interpolate all string values in a JSON Value.
pub fn interpolate_value(
    value: &mut Value,
    ctx: &InterpolationContext<'_>,
) -> anyhow::Result<()> {
    match value {
        Value::String(s) => {
            *s = interpolate(s, ctx)?;
        }
        Value::Array(arr) => {
            for item in arr {
                interpolate_value(item, ctx)?;
            }
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                interpolate_value(v, ctx)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Interpolate all string values in a config map.
pub fn interpolate_config(
    config: &mut HashMap<String, Value>,
    ctx: &InterpolationContext<'_>,
) -> anyhow::Result<()> {
    for v in config.values_mut() {
        interpolate_value(v, ctx)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Go template function registration (Packer FuncMap equivalent)
// ---------------------------------------------------------------------------

/// Register all Packer-compatible template functions.
/// Mirrors Packer's `template/interpolate/funcs.go` FuncMap.
fn register_packer_funcs(tmpl: &mut Template) {
    tmpl.add_func("user", fn_user);
    tmpl.add_func("env", fn_env);
    tmpl.add_func("timestamp", fn_timestamp);
    tmpl.add_func("uuid", fn_uuid);
    tmpl.add_func("isotime", fn_isotime);
    tmpl.add_func("strftime", fn_strftime);
    tmpl.add_func("build_name", fn_build_name);
    tmpl.add_func("build_type", fn_build_type);
    tmpl.add_func("template_dir", fn_template_dir);
    tmpl.add_func("pwd", fn_pwd);
    tmpl.add_func("packer_version", fn_version);
    tmpl.add_func("igata_version", fn_version);
    tmpl.add_func("upper", fn_upper);
    tmpl.add_func("lower", fn_lower);
    tmpl.add_func("replace", fn_replace);
    tmpl.add_func("replace_all", fn_replace_all);
    tmpl.add_func("split", fn_split);
    tmpl.add_func("clean_resource_name", fn_clean_resource_name);
}

// -- Packer function implementations (plain fn pointers) --

fn fn_user(args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    let key = str_arg(args, 0, "user")?;
    with_tls_ctx(|ctx| {
        ctx.variables
            .get(&key)
            .map(|v| GtmplValue::String(v.clone()))
            .ok_or_else(|| FuncError::Generic(format!("undefined variable: {key}")))
    })
}

fn fn_env(args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    let key = str_arg(args, 0, "env")?;
    std::env::var(&key)
        .map(GtmplValue::String)
        .map_err(|_| FuncError::Generic(format!("environment variable not set: {key}")))
}

fn fn_timestamp(_args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    Ok(GtmplValue::String(chrono::Utc::now().timestamp().to_string()))
}

fn fn_uuid(_args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    Ok(GtmplValue::String(uuid::Uuid::new_v4().to_string()))
}

fn fn_isotime(args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    let now = chrono::Utc::now();
    if let Some(GtmplValue::String(fmt)) = args.first() {
        Ok(GtmplValue::String(now.format(fmt).to_string()))
    } else {
        Ok(GtmplValue::String(now.to_rfc3339()))
    }
}

fn fn_strftime(args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    let fmt = str_arg(args, 0, "strftime")?;
    Ok(GtmplValue::String(chrono::Utc::now().format(&fmt).to_string()))
}

fn fn_build_name(_args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    with_tls_ctx(|ctx| Ok(GtmplValue::String(ctx.build_name.clone())))
}

fn fn_build_type(_args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    with_tls_ctx(|ctx| Ok(GtmplValue::String(ctx.build_type.clone())))
}

fn fn_template_dir(_args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    with_tls_ctx(|ctx| Ok(GtmplValue::String(ctx.template_dir.clone())))
}

fn fn_pwd(_args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    std::env::current_dir()
        .map(|p| GtmplValue::String(p.display().to_string()))
        .map_err(|e| FuncError::Generic(format!("failed to get pwd: {e}")))
}

fn fn_version(_args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    Ok(GtmplValue::String(env!("CARGO_PKG_VERSION").to_string()))
}

fn fn_upper(args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    Ok(GtmplValue::String(str_arg(args, 0, "upper")?.to_uppercase()))
}

fn fn_lower(args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    Ok(GtmplValue::String(str_arg(args, 0, "lower")?.to_lowercase()))
}

fn fn_replace(args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    if args.len() >= 3 {
        let old = str_arg(args, 0, "replace")?;
        let new = str_arg(args, 1, "replace")?;
        let input = str_arg(args, 2, "replace")?;
        Ok(GtmplValue::String(input.replace(&old, &new)))
    } else {
        Err(FuncError::Generic("replace requires 3 args: old new str".into()))
    }
}

fn fn_replace_all(args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    fn_replace(args) // same behavior — Go's replace_all is just replace with n=-1
}

fn fn_split(args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    if args.len() >= 3 {
        let delim = str_arg(args, 0, "split")?;
        let idx = match &args[1] {
            GtmplValue::String(s) => s.parse::<usize>().unwrap_or(0),
            GtmplValue::Number(n) => n.to_string().parse::<usize>().unwrap_or(0),
            _ => 0,
        };
        let input = str_arg(args, 2, "split")?;
        let parts: Vec<&str> = input.split(&delim).collect();
        Ok(GtmplValue::String(parts.get(idx).unwrap_or(&"").to_string()))
    } else {
        Err(FuncError::Generic("split requires 3 args: delim index str".into()))
    }
}

fn fn_clean_resource_name(args: &[GtmplValue]) -> Result<GtmplValue, FuncError> {
    let s = str_arg(args, 0, "clean_resource_name")?;
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    Ok(GtmplValue::String(cleaned))
}

// ---------------------------------------------------------------------------
// Dot-context data for the Go template engine
// ---------------------------------------------------------------------------

fn build_dot_data(ctx: &InterpolationContext<'_>) -> GtmplValue {
    let mut map: HashMap<String, GtmplValue> = HashMap::new();

    map.insert("Name".into(), GtmplValue::String(ctx.build_name.to_string()));
    map.insert("BuildName".into(), GtmplValue::String(ctx.build_name.to_string()));
    map.insert("BuilderType".into(), GtmplValue::String(ctx.build_type.to_string()));

    for (key, val) in &ctx.dot_context {
        let field = key.strip_prefix('.').unwrap_or(key);
        map.insert(field.to_string(), GtmplValue::String(val.clone()));
    }

    GtmplValue::Object(map)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn str_arg(args: &[GtmplValue], idx: usize, func: &str) -> Result<String, FuncError> {
    args.get(idx)
        .and_then(|v| match v {
            GtmplValue::String(s) => Some(s.clone()),
            other => Some(format!("{other}")),
        })
        .ok_or_else(|| FuncError::Generic(format!("{func}: missing arg at index {idx}")))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx(vars: &Variables) -> InterpolationContext<'_> {
        InterpolationContext {
            variables: vars,
            build_name: "test-build",
            build_type: "null",
            template_dir: Some("/tmp/test"),
            dot_context: HashMap::new(),
        }
    }

    fn test_ctx_with_dots(
        vars: &Variables,
        dots: HashMap<String, String>,
    ) -> InterpolationContext<'_> {
        InterpolationContext {
            variables: vars,
            build_name: "test-build",
            build_type: "null",
            template_dir: Some("/tmp/test"),
            dot_context: dots,
        }
    }

    #[test]
    fn test_user_variable() {
        let mut vars = Variables::new();
        vars.insert("name".into(), "hello".into());
        let ctx = test_ctx(&vars);
        let result = interpolate("{{user `name`}}", &ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_env_variable() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        unsafe { std::env::set_var("IGATA_TEST_VAR", "world") };
        let result = interpolate("{{env `IGATA_TEST_VAR`}}", &ctx).unwrap();
        assert_eq!(result, "world");
        unsafe { std::env::remove_var("IGATA_TEST_VAR") };
    }

    #[test]
    fn test_timestamp() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{timestamp}}", &ctx).unwrap();
        assert!(result.parse::<i64>().is_ok());
    }

    #[test]
    fn test_uuid() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{uuid}}", &ctx).unwrap();
        assert!(uuid::Uuid::parse_str(&result).is_ok());
    }

    #[test]
    fn test_build_name() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{build_name}}", &ctx).unwrap();
        assert_eq!(result, "test-build");
    }

    #[test]
    fn test_multiple_interpolations() {
        let mut vars = Variables::new();
        vars.insert("a".into(), "1".into());
        vars.insert("b".into(), "2".into());
        let ctx = test_ctx(&vars);
        let result = interpolate("{{user `a`}}-{{user `b`}}", &ctx).unwrap();
        assert_eq!(result, "1-2");
    }

    #[test]
    fn test_no_interpolation() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("plain text", &ctx).unwrap();
        assert_eq!(result, "plain text");
    }

    #[test]
    fn test_pipe_upper() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{ \"hello\" | upper }}", &ctx).unwrap();
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn test_pipe_lower() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{ \"HELLO\" | lower }}", &ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_pipe_user_to_upper() {
        let mut vars = Variables::new();
        vars.insert("name".into(), "hello".into());
        let ctx = test_ctx(&vars);
        let result = interpolate("{{ user `name` | upper }}", &ctx).unwrap();
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn test_dot_vars() {
        let vars = Variables::new();
        let dots = HashMap::from([
            (".Vars".to_string(), "FOO=bar BAZ=qux".to_string()),
            (".Path".to_string(), "/tmp/script.sh".to_string()),
        ]);
        let ctx = test_ctx_with_dots(&vars, dots);
        let result = interpolate(
            "chmod +x {{.Path}} && {{.Vars}} {{.Path}}",
            &ctx,
        )
        .unwrap();
        assert_eq!(
            result,
            "chmod +x /tmp/script.sh && FOO=bar BAZ=qux /tmp/script.sh"
        );
    }

    #[test]
    fn test_dot_build_name() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{.BuildName}}", &ctx).unwrap();
        assert_eq!(result, "test-build");
    }

    #[test]
    fn test_dot_builder_type() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{.BuilderType}}", &ctx).unwrap();
        assert_eq!(result, "null");
    }

    #[test]
    fn test_go_conditional() {
        let mut vars = Variables::new();
        vars.insert("debug".into(), "true".into());
        let ctx = test_ctx(&vars);
        let result = interpolate(
            "{{if eq (user `debug`) \"true\"}}DEBUG{{else}}PROD{{end}}",
            &ctx,
        )
        .unwrap();
        assert_eq!(result, "DEBUG");
    }

    #[test]
    fn test_clean_resource_name() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{clean_resource_name `My Image Name!`}}", &ctx).unwrap();
        assert_eq!(result, "my-image-name-");
    }

    #[test]
    fn test_template_dir() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{template_dir}}", &ctx).unwrap();
        assert_eq!(result, "/tmp/test");
    }

    #[test]
    fn test_packer_version() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{packer_version}}", &ctx).unwrap();
        assert_eq!(result, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_igata_version() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{igata_version}}", &ctx).unwrap();
        assert_eq!(result, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_build_type() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{build_type}}", &ctx).unwrap();
        assert_eq!(result, "null");
    }

    #[test]
    fn test_pwd() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{pwd}}", &ctx).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_isotime_default() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{isotime}}", &ctx).unwrap();
        // Should be an RFC3339 timestamp
        assert!(result.contains('T'));
    }

    #[test]
    fn test_isotime_formatted() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{isotime \"%Y-%m-%d\"}}", &ctx).unwrap();
        // Should be like 2026-03-27
        assert_eq!(result.len(), 10);
        assert!(result.contains('-'));
    }

    #[test]
    fn test_strftime() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{strftime \"%Y\"}}", &ctx).unwrap();
        assert_eq!(result.len(), 4);
        assert!(result.parse::<u32>().is_ok());
    }

    #[test]
    fn test_replace_direct() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result =
            interpolate("{{replace \"world\" \"rust\" \"hello world\"}}", &ctx).unwrap();
        assert_eq!(result, "hello rust");
    }

    #[test]
    fn test_replace_all_direct() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result =
            interpolate("{{replace_all \"o\" \"0\" \"foo boo\"}}", &ctx).unwrap();
        assert_eq!(result, "f00 b00");
    }

    #[test]
    fn test_split_direct() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{split \"-\" 1 \"a-b-c\"}}", &ctx).unwrap();
        assert_eq!(result, "b");
    }

    #[test]
    fn test_split_index_zero() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{split \"-\" 0 \"a-b-c\"}}", &ctx).unwrap();
        assert_eq!(result, "a");
    }

    #[test]
    fn test_upper_direct() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{upper \"hello\"}}", &ctx).unwrap();
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn test_lower_direct() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{lower \"HELLO\"}}", &ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_user_double_quoted() {
        let mut vars = Variables::new();
        vars.insert("x".into(), "val".into());
        let ctx = test_ctx(&vars);
        let result = interpolate("{{user \"x\"}}", &ctx).unwrap();
        assert_eq!(result, "val");
    }

    #[test]
    fn test_user_undefined_variable_error() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{user `nonexistent`}}", &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_env_undefined_error() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{env `IGATA_DEFINITELY_NOT_SET_XYZ`}}", &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_go_conditional_else_branch() {
        let mut vars = Variables::new();
        vars.insert("debug".into(), "false".into());
        let ctx = test_ctx(&vars);
        let result = interpolate(
            "{{if eq (user `debug`) \"true\"}}DEBUG{{else}}PROD{{end}}",
            &ctx,
        )
        .unwrap();
        assert_eq!(result, "PROD");
    }

    #[test]
    fn test_mixed_text_and_interpolation() {
        let mut vars = Variables::new();
        vars.insert("name".into(), "test".into());
        let ctx = test_ctx(&vars);
        let result =
            interpolate("ami-{{user `name`}}-{{timestamp}}", &ctx).unwrap();
        assert!(result.starts_with("ami-test-"));
        // Check that the timestamp part is numeric
        let ts_part = result.strip_prefix("ami-test-").unwrap();
        assert!(ts_part.parse::<i64>().is_ok());
    }

    #[test]
    fn test_interpolate_value_nested_object() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let mut value = serde_json::json!({
            "outer": {
                "inner": "{{build_name}}"
            },
            "list": ["{{build_type}}", "static"]
        });
        interpolate_value(&mut value, &ctx).unwrap();
        assert_eq!(value["outer"]["inner"], "test-build");
        assert_eq!(value["list"][0], "null");
        assert_eq!(value["list"][1], "static");
    }

    #[test]
    fn test_interpolate_config_map() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let mut config = HashMap::new();
        config.insert("name".into(), Value::String("{{build_name}}".into()));
        config.insert("count".into(), Value::Number(42.into()));
        interpolate_config(&mut config, &ctx).unwrap();
        assert_eq!(config["name"], Value::String("test-build".into()));
        assert_eq!(config["count"], Value::Number(42.into())); // numbers untouched
    }

    #[test]
    fn test_uuid_uniqueness() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let a = interpolate("{{uuid}}", &ctx).unwrap();
        let b = interpolate("{{uuid}}", &ctx).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn test_dot_name() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate("{{.Name}}", &ctx).unwrap();
        assert_eq!(result, "test-build");
    }

    #[test]
    fn test_pipe_chain_three_levels() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        // upper then lower should return lowercase
        let result =
            interpolate("{{ \"Hello\" | upper | lower }}", &ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_clean_resource_name_special_chars() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate(
            "{{clean_resource_name `My@Image#2024!`}}",
            &ctx,
        )
        .unwrap();
        assert_eq!(result, "my-image-2024-");
    }

    #[test]
    fn test_clean_resource_name_preserves_hyphens_underscores() {
        let vars = Variables::new();
        let ctx = test_ctx(&vars);
        let result = interpolate(
            "{{clean_resource_name `my-image_v1`}}",
            &ctx,
        )
        .unwrap();
        assert_eq!(result, "my-image_v1");
    }
}
