# igata — Nix-first Machine Image Builder

> **★★★ CSE / Knowable Construction.** This repo operates under **Constructive Substrate Engineering** — canonical specification at [`pleme-io/theory/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md`](https://github.com/pleme-io/theory/blob/main/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md). The Compounding Directive (operational rules: solve once, load-bearing fixes only, idiom-first, models stay current, direction beats velocity) is in the org-level pleme-io/CLAUDE.md ★★★ section. Read both before non-trivial changes.


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
| Amazon Import | None | `ami-forge` (path dep) |

#### Amazon Import (`amazon-import`)

Uploads a **pre-built** Nix disk image (a `nixos-generators` `.raw`/`.img`/`.vhd`
output — already fully configured, no provisioning step) to S3, imports it as
an AMI via the `vmimport` service role, tags it, and updates an SSM parameter
with the resulting AMI ID. This is the amazon-ebs builder's opposite number:
amazon-ebs launches an instance and provisions it over SSH (the classic Packer
pattern, kept for Packer-template compatibility); amazon-import never launches
an instance at all — there is nothing to provision, so [`run()`](src/builder/amazon_import.rs)
returns no communicator, same as the Null builder. `igata build <template>`
is the single front door for this pipeline — no separate manual
`ami-forge build` invocation is needed afterward.

The upload/import/tag/SSM-update sequence is not re-implemented in igata: it
calls `ami_forge::build::run()` in-process, reusing ami-forge's own proven
pipeline (path dependency on the sibling `ami-forge` checkout — see
`Cargo.toml`).

Config fields:

| Field | Required | Default | Meaning |
|-------|----------|---------|---------|
| `image` | yes | — | Path to the disk image, or the directory containing it (`*.raw`/`*.img`/`*.vhd`) |
| `bucket` | yes | — | S3 bucket used for the transit upload (deleted after import) |
| `ami_name` | yes | — | Constant AMI name — used for rotation (deregister the old AMI with this name) and tagging |
| `ssm` | yes | — | SSM parameter path updated with the new AMI ID |
| `region` | no | `us-east-1` | AWS region |
| `role_name` | no | `<ami_name>-vmimport` | IAM role for the `vmimport` service |
| `timeout` | no | `1800` | Import-task polling timeout, in seconds |

Example shikumi YAML template:

```yaml
builders:
  - type: amazon-import
    image: /nix/store/.../nixos-camelot-portao.raw
    bucket: pleme-ami-transit
    ami_name: camelot-portao
    ssm: /pleme/camelot/portao/ami-id
    region: us-east-1
    timeout: 1800
```

Equivalent Packer-JSON-shaped template:

```json
{
  "builders": [
    {
      "type": "amazon-import",
      "image": "/nix/store/.../nixos-camelot-portao.raw",
      "bucket": "pleme-ami-transit",
      "ami_name": "camelot-portao",
      "ssm": "/pleme/camelot/portao/ami-id",
      "region": "us-east-1",
      "timeout": 1800
    }
  ]
}
```

**Known gap (2026-07-16, tier-honest — do not round up):** `cargo build` /
`cargo test` are fully green with the `ami-forge` path dependency (28/28 +
198/198 tests). `gen build .` (the step that regenerates the committed
`Cargo.gen.lock` delta and feeds `nix build` via IFD) currently fails:

```
gen: error: failed to read .: gen build (multi-target): spec violates algorithmic invariants (1 issues):
[{"rule":"unresolved-dep","from":"pleme-igata-0.1.2","dep_name":"ami_forge","missing_key":"ami-forge-0.2.0"}]
```

`gen` (v0.1.18) does correctly promote a path dependency on a sibling
git-backed crate to a git-pinned, sha256-fetchable coordinate (it derives
this from `ami-forge`'s own `repository` field + the checkout's current
commit — confirmed working once that commit is pushed). The failure is a
narrower internal key mismatch in its multi-target resolver: it looks the
promoted dependency up once under the Rust identifier form (`ami_forge`,
underscored) and once under the Cargo package-name-version form
(`ami-forge-0.2.0`, hyphenated), and the two don't reconcile. This reproduced
identically with and without an explicit `[lib]` stanza in `ami-forge`'s
`Cargo.toml`, so it isn't caused by anything in this repo or in ami-forge —
it's a `pleme-io/gen` gap, out of scope to patch from here. `Cargo.gen.lock`
is left untouched (still reflecting the pre-`amazon-import` dependency set)
rather than hand-edited or force-regenerated; `nix build` for igata is
consequently unverified until this is fixed upstream in `gen`.

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
