use igata::{Context, Engine, Manifest, Source, Syntax};
use std::io::Write;
use tempfile::NamedTempFile;

// ── Basic rendering ────────────────────────────────────────────────────

#[test]
fn render_literal_variables() {
    let engine = Engine::new();
    let ctx = Context::from([("name", "world"), ("count", "42")]);
    let result = engine
        .render_str("Hello [= name =]! Count: [= count =]", &ctx)
        .unwrap();
    assert_eq!(result, "Hello world! Count: 42");
}

#[test]
fn render_empty_template() {
    let engine = Engine::new();
    let result = engine.render_str("", &Context::new()).unwrap();
    assert_eq!(result, "");
}

#[test]
fn render_no_variables() {
    let engine = Engine::new();
    let result = engine
        .render_str("plain text, no variables", &Context::new())
        .unwrap();
    assert_eq!(result, "plain text, no variables");
}

#[test]
fn render_preserves_whitespace() {
    let engine = Engine::new();
    let ctx = Context::from([("val", "x")]);
    let result = engine
        .render_str("  [= val =]  \n  [= val =]  ", &ctx)
        .unwrap();
    assert_eq!(result, "  x  \n  x  ");
}

// ── Context builder ────────────────────────────────────────────────────

#[test]
fn context_builder_literal() {
    let ctx = Context::builder()
        .literal("a", "1")
        .literal("b", "2")
        .build();
    let resolved = ctx.resolve().unwrap();
    assert_eq!(resolved["a"], "1");
    assert_eq!(resolved["b"], "2");
}

#[test]
fn context_builder_file() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "secret_value").unwrap();

    let ctx = Context::builder()
        .file("token", tmp.path().to_str().unwrap())
        .build();
    let resolved = ctx.resolve().unwrap();
    assert_eq!(resolved["token"], "secret_value");
}

#[test]
fn context_builder_file_trims_trailing_whitespace() {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "  value  \n\n").unwrap();

    let ctx = Context::builder()
        .file("v", tmp.path().to_str().unwrap())
        .build();
    let resolved = ctx.resolve().unwrap();
    assert_eq!(resolved["v"], "  value");
}

#[test]
fn context_builder_env() {
    // SAFETY: test-only, single-threaded test binary.
    unsafe { std::env::set_var("IGATA_TEST_VAR", "hello_igata") };
    let ctx = Context::builder().env("val", "IGATA_TEST_VAR").build();
    let resolved = ctx.resolve().unwrap();
    assert_eq!(resolved["val"], "hello_igata");
    unsafe { std::env::remove_var("IGATA_TEST_VAR") };
}

#[test]
fn context_env_not_set_errors() {
    let ctx = Context::builder()
        .env("missing", "IGATA_NONEXISTENT_VAR_12345")
        .build();
    let err = ctx.resolve().unwrap_err();
    assert!(err.to_string().contains("not set"));
}

#[test]
fn context_file_not_found_errors() {
    let ctx = Context::builder()
        .file("missing", "/nonexistent/path/to/file")
        .build();
    let err = ctx.resolve().unwrap_err();
    assert!(err.to_string().contains("failed to read"));
}

#[test]
fn context_from_array() {
    let ctx = Context::from([("x", "1"), ("y", "2")]);
    let resolved = ctx.resolve().unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved["x"], "1");
    assert_eq!(resolved["y"], "2");
}

// ── File rendering ─────────────────────────────────────────────────────

#[test]
fn render_template_file() {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "db_url = [= url =]\npool = [= pool =]").unwrap();

    let engine = Engine::new();
    let ctx = Context::from([("url", "postgres://localhost/app"), ("pool", "10")]);
    let result = engine.render_file(tmp.path(), &ctx).unwrap();
    assert_eq!(result, "db_url = postgres://localhost/app\npool = 10");
}

#[test]
fn render_to_file_creates_output() {
    let mut tmpl = NamedTempFile::new().unwrap();
    write!(tmpl, "key = [= value =]").unwrap();

    let out = NamedTempFile::new().unwrap();
    let out_path = out.path().to_path_buf();

    let engine = Engine::new();
    let ctx = Context::from([("value", "secret123")]);
    engine
        .render_to_file(tmpl.path(), &out_path, &ctx, 0o600)
        .unwrap();

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert_eq!(content, "key = secret123");
}

// ── Template logic ─────────────────────────────────────────────────────

#[test]
fn render_conditional() {
    let engine = Engine::new();
    let template = "[% if tls == \"true\" %]tls = on[% endif %]";
    let ctx = Context::from([("tls", "true")]);
    let result = engine.render_str(template, &ctx).unwrap();
    assert_eq!(result, "tls = on");
}

#[test]
fn render_conditional_false() {
    let engine = Engine::new();
    let template = "[% if tls == \"true\" %]tls = on[% endif %]";
    let ctx = Context::from([("tls", "false")]);
    let result = engine.render_str(template, &ctx).unwrap();
    assert_eq!(result, "");
}

#[test]
fn render_for_loop() {
    let mut env = minijinja::Environment::new();
    env.set_syntax(Syntax::default().to_config().unwrap());
    let tmpl = env
        .template_from_str("[% for item in items %][= item =]\n[% endfor %]")
        .unwrap();
    let result = tmpl
        .render(minijinja::context! { items => ["a", "b", "c"] })
        .unwrap();
    assert_eq!(result, "a\nb\nc\n");
}

#[test]
fn render_default_filter() {
    let engine = Engine::new();
    let template = "val = [= missing | default(\"fallback\") =]";
    let result = engine.render_str(template, &Context::new()).unwrap();
    assert_eq!(result, "val = fallback");
}

// ── Custom syntax ──────────────────────────────────────────────────────

#[test]
fn custom_variable_delimiters() {
    let engine = Engine::builder()
        .variable_delimiters("<<", ">>")
        .build()
        .unwrap();
    let ctx = Context::from([("name", "custom")]);
    let result = engine.render_str("Hello << name >>!", &ctx).unwrap();
    assert_eq!(result, "Hello custom!");
}

#[test]
fn custom_all_delimiters() {
    let engine = Engine::builder()
        .variable_delimiters("${", "}")
        .block_delimiters("<%", "%>")
        .comment_delimiters("<!", "!>")
        .build()
        .unwrap();
    let ctx = Context::from([("x", "1")]);
    let result = engine.render_str("val = ${x}", &ctx).unwrap();
    assert_eq!(result, "val = 1");
}

#[test]
fn default_syntax_avoids_nix_conflict() {
    let engine = Engine::new();
    let template = "nix = ${foo}\nigata = [= bar =]";
    let ctx = Context::from([("bar", "ok")]);
    let result = engine.render_str(template, &ctx).unwrap();
    assert_eq!(result, "nix = ${foo}\nigata = ok");
}

// ── Syntax config ──────────────────────────────────────────────────────

#[test]
fn syntax_default_values() {
    let s = Syntax::default();
    assert_eq!(s.variable, ("[=".to_string(), "=]".to_string()));
    assert_eq!(s.block, ("[%".to_string(), "%]".to_string()));
    assert_eq!(s.comment, ("[#".to_string(), "#]".to_string()));
}

#[test]
fn syntax_to_config_succeeds() {
    let s = Syntax::default();
    assert!(s.to_config().is_ok());
}

#[test]
fn syntax_serde_roundtrip() {
    let original = Syntax::default();
    let json = serde_json::to_string(&original).unwrap();
    let restored: Syntax = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.variable, original.variable);
    assert_eq!(restored.block, original.block);
    assert_eq!(restored.comment, original.comment);
}

// ── Manifest ───────────────────────────────────────────────────────────

#[test]
fn manifest_load_and_render() {
    let mut tmpl = NamedTempFile::new().unwrap();
    write!(tmpl, "token = [= token =]\nhost = [= host =]").unwrap();

    let out = NamedTempFile::new().unwrap();
    let out_path = out.path().to_path_buf();

    let mut secret = NamedTempFile::new().unwrap();
    write!(secret, "s3cr3t").unwrap();

    let manifest_json = serde_json::json!({
        "syntax": {},
        "templates": {
            "app-config": {
                "source": tmpl.path().to_str().unwrap(),
                "target": out_path.to_str().unwrap(),
                "context": {
                    "variables": {
                        "token": { "type": "file", "path": secret.path().to_str().unwrap() },
                        "host": { "type": "literal", "value": "db.example.com" }
                    }
                }
            }
        }
    });

    let mut manifest_file = NamedTempFile::new().unwrap();
    serde_json::to_writer(&mut manifest_file, &manifest_json).unwrap();

    let manifest = Manifest::load(manifest_file.path()).unwrap();
    let engine = Engine::new();
    let report = engine.render_manifest(&manifest).unwrap();

    assert_eq!(report.rendered.len(), 1);
    assert_eq!(report.rendered[0], "app-config");

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert_eq!(content, "token = s3cr3t\nhost = db.example.com");
}

#[test]
fn manifest_with_env_source() {
    // SAFETY: test-only, single-threaded.
    unsafe { std::env::set_var("IGATA_TEST_MANIFEST_VAR", "from_env") };

    let mut tmpl = NamedTempFile::new().unwrap();
    write!(tmpl, "val = [= myvar =]").unwrap();

    let out = NamedTempFile::new().unwrap();
    let out_path = out.path().to_path_buf();

    let manifest_json = serde_json::json!({
        "templates": {
            "env-test": {
                "source": tmpl.path().to_str().unwrap(),
                "target": out_path.to_str().unwrap(),
                "context": {
                    "variables": {
                        "myvar": { "type": "env", "name": "IGATA_TEST_MANIFEST_VAR" }
                    }
                }
            }
        }
    });

    let mut manifest_file = NamedTempFile::new().unwrap();
    serde_json::to_writer(&mut manifest_file, &manifest_json).unwrap();

    let manifest = Manifest::load(manifest_file.path()).unwrap();
    let engine = Engine::new();
    let report = engine.render_manifest(&manifest).unwrap();

    assert_eq!(report.rendered.len(), 1);
    let content = std::fs::read_to_string(&out_path).unwrap();
    assert_eq!(content, "val = from_env");

    unsafe { std::env::remove_var("IGATA_TEST_MANIFEST_VAR") };
}

#[test]
fn manifest_custom_syntax() {
    let mut tmpl = NamedTempFile::new().unwrap();
    write!(tmpl, "val = << x >>").unwrap();

    let out = NamedTempFile::new().unwrap();
    let out_path = out.path().to_path_buf();

    let manifest_json = serde_json::json!({
        "syntax": {
            "variable": ["<<", ">>"],
            "block": ["<%", "%>"],
            "comment": ["<!", "!>"]
        },
        "templates": {
            "custom": {
                "source": tmpl.path().to_str().unwrap(),
                "target": out_path.to_str().unwrap(),
                "context": {
                    "variables": {
                        "x": { "type": "literal", "value": "42" }
                    }
                }
            }
        }
    });

    let mut manifest_file = NamedTempFile::new().unwrap();
    serde_json::to_writer(&mut manifest_file, &manifest_json).unwrap();

    let manifest = Manifest::load(manifest_file.path()).unwrap();
    let engine = Engine::new();
    let report = engine.render_manifest(&manifest).unwrap();

    assert_eq!(report.rendered.len(), 1);
    let content = std::fs::read_to_string(&out_path).unwrap();
    assert_eq!(content, "val = 42");
}

// ── Real-world patterns ────────────────────────────────────────────────

#[test]
fn kubeconfig_template() {
    let engine = Engine::new();
    let template = r#"apiVersion: v1
kind: Config
users:
- name: plo
  user:
    token: [= plo_token =]
- name: zek
  user:
    client-certificate-data: [= zek_cert =]
    client-key-data: [= zek_key =]"#;

    let ctx = Context::from([
        ("plo_token", "tok-abc123"),
        ("zek_cert", "Y2VydA=="),
        ("zek_key", "a2V5"),
    ]);

    let result = engine.render_str(template, &ctx).unwrap();
    assert!(result.contains("token: tok-abc123"));
    assert!(result.contains("client-certificate-data: Y2VydA=="));
}

#[test]
fn cargo_credentials_template() {
    let engine = Engine::new();
    let template = "[registry]\ntoken = \"[= crates_token =]\"";
    let ctx = Context::from([("crates_token", "cio-secret")]);
    let result = engine.render_str(template, &ctx).unwrap();
    assert_eq!(result, "[registry]\ntoken = \"cio-secret\"");
}

#[test]
fn wireguard_config_with_conditional() {
    let engine = Engine::new();
    let template = "[Interface]\nPrivateKey = [= private_key =]\n[% if keepalive == \"true\" %]PersistentKeepalive = 25\n[% endif %]";
    let ctx = Context::from([("private_key", "AAAA..."), ("keepalive", "true")]);
    let result = engine.render_str(template, &ctx).unwrap();
    assert!(result.contains("PersistentKeepalive = 25"));
    assert!(result.contains("PrivateKey = AAAA..."));
}

// ── Source serde ───────────────────────────────────────────────────────

#[test]
fn source_literal_serde() {
    let source = Source::Literal { value: "hello".into() };
    let json = serde_json::to_string(&source).unwrap();
    let restored: Source = serde_json::from_str(&json).unwrap();
    if let Source::Literal { value } = restored {
        assert_eq!(value, "hello");
    } else {
        panic!("expected Literal");
    }
}

#[test]
fn source_file_serde() {
    let source = Source::File { path: "/run/secrets/token".into() };
    let json = serde_json::to_string(&source).unwrap();
    assert!(json.contains("\"type\":\"file\""));
}

#[test]
fn source_env_serde() {
    let source = Source::Env { name: "HOME".into() };
    let json = serde_json::to_string(&source).unwrap();
    assert!(json.contains("\"type\":\"env\""));
}

// ── Error quality ──────────────────────────────────────────────────────

#[test]
fn missing_variable_renders_empty_by_default() {
    // MiniJinja renders undefined variables as empty string (lenient mode).
    let engine = Engine::new();
    let result = engine
        .render_str("[= nonexistent =]", &Context::new())
        .unwrap();
    assert_eq!(result, "");
}

#[test]
fn invalid_template_syntax_error() {
    let engine = Engine::new();
    let err = engine.render_str("[% if %]", &Context::new()).unwrap_err();
    assert!(err.to_string().contains("template error"));
}
