# igata (鋳型)

Nix-first machine-image builder that consumes **Packer-compatible JSON
templates**.

Packer's template format is the input contract, so existing templates port
without rewriting; the build itself is Nix-first rather than shelling out to a
provisioner chain.

## Modules

`build` · `builder` · `communicator` · `config` · `display` · `error`

## Usage

```toml
[dependencies]
pleme-igata = "0.1"
```

## License

MIT — see [LICENSE](./LICENSE).
