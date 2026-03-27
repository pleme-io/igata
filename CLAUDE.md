# igata — Nix-first Machine Image Builder

Rust-native, Nix-first machine image builder. Fully compatible with Packer's JSON
template schema with additional shikumi YAML support. Named igata (casting mold).

## Quick Reference

```bash
igata build template.json                    # Build from Packer JSON
igata build template.yaml                    # Build from shikumi YAML
igata build --only docker template.json      # Build specific builder
igata validate template.json                 # Validate template
igata inspect template.json                  # Show template summary
igata version                                # Show version
```

## Architecture

```
Packer JSON ──┐
              ├──→ Template IR (core types) ──→ Build Pipeline
Shikumi YAML ─┘         │                          │
                   ┌─────┴─────┐        ┌──────────┼──────────┐
              BuilderConfig  ProvisionerConfig    Builder    Provisioner
              PostProcessorConfig                    │
                                              Communicator
                                          (SSH / Docker / None)
```

### Core Data Structure Philosophy

The `Template` struct in `src/template.rs` is the **canonical IR** — the single
source of truth that all input formats deserialize into and all pipeline stages
consume. Follows the dq pattern: serde-based format adapters converging to
universal types. No custom parsers — format-specific serde libraries handle parsing.

### Core Types (`src/template.rs`)

| Type | Purpose |
|------|---------|
| `Template` | Complete build specification — variables, builders, provisioners, post-processors |
| `BuilderConfig` | Machine spec with typed SSH/WinRM communicator fields + flattened builder config |
| `ProvisionerConfig` | Step config with only/except/override/pause_before/max_retries + flattened fields |
| `PostProcessorEntry` | String shorthand, single object, or pipeline array (all 3 Packer formats) |
| `PostProcessorConfig` | Post-processor with only/except/keep_input_artifact + flattened fields |

### Core Traits (`src/traits.rs`)

| Trait | Purpose |
|-------|---------|
| `Builder` | Machine lifecycle (prepare → run → artifact → cleanup) |
| `Provisioner` | Configuration inside the machine (shell, file, etc.) |
| `PostProcessor` | Transform artifacts after build (checksum, compress, etc.) |
| `Communicator` | Upload/download/exec inside the machine (SSH, docker exec) |
| `Registry` | Factory registry — creates trait objects by type name string |

### Input Format Support

| Format | Extension | Parser | Use case |
|--------|-----------|--------|----------|
| Packer JSON | `.json` | `serde_json` | Full Packer compatibility |
| Shikumi YAML | `.yaml`/`.yml` | `serde_yaml_ng` | Nix-native, human-friendly |

Both formats produce identical `Template` IR. Auto-detected by file extension.

### Packer Compatibility

Full support for Packer JSON template schema including:
- All top-level keys: `builders`, `provisioners`, `post-processors`, `variables`, `sensitive-variables`, `description`, `min_packer_version`
- `_comment` convention (underscore-prefixed root keys ignored)
- Post-processor formats: string shorthand, single object, pipeline array
- Variable precedence: CLI `-var` > `-var-file` > `PKR_VAR_*` env > defaults
- All communicator fields: SSH (30+ fields), WinRM
- Interpolation functions: `user`, `env`, `timestamp`, `uuid`, `isotime`, `strftime`, `build_name`, `build_type`, `template_dir`, `pwd`, `upper`, `lower`, `replace`, `replace_all`, `split`, `clean_resource_name`, `packer_version`
- Provisioner: `only`/`except` filtering, `override`, `pause_before`, `max_retries`, `timeout`
- Post-processor: `keep_input_artifact`, pipeline chaining

### Builders

| Builder | Communicator | Key deps |
|---------|-------------|----------|
| Null | None | — |
| Docker | docker exec (bollard) | bollard |
| QEMU | SSH (russh) | russh, tokio::process |
| Amazon EBS | SSH (russh) | aws-sdk-ec2 |

### Config

Shikumi config at `~/.config/igata/igata.yaml` with `IGATA_` env prefix.
Provides defaults for SSH timeout, username, AWS region, Docker host, QEMU binary.

## Build

```bash
cargo build
cargo test
nix build
```

## Conventions

- Edition 2021, MIT license
- `thiserror` for error types, `anyhow` for propagation
- `async-trait` for all async traits
- `colored` for terminal output
- Packer JSON schema compatibility is the top priority
- Core data structures in `template.rs` are the single source of truth
- serde-based parsing (no custom parsers) — follows dq pattern
