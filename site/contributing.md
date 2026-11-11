# Contributing and publishing

## Repository map

| Path | Contents |
| --- | --- |
| `src/` | Rust compiler, integrations and runtime APIs |
| `tests/` | Native integration regressions |
| `examples/` | Complete diagram fixtures |
| `web/` | Browser authoring, worker and spatial demos |
| `site/` | This VitePress documentation site |
| `docs/` | Canonical architecture/performance/verification notes and supplied requirements |
| `extensions/` | VS Code and Neovim adapters |

## Native checks

```sh
cargo fmt --check
cargo clippy --locked --all-features --all-targets -- -D warnings
cargo test --locked --all-features
cargo bench --bench pipeline
```

Add behavioral regressions for source preservation, constraints and geometry when changing those invariants. Preserve the exact original requirement files; update implementation notes separately.

## Build this site

```sh
npm ci
cargo build --locked --all-features
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
scripts/build-wasm.sh core
scripts/build-wasm.sh runtime
RUSTDOCFLAGS='-D warnings' cargo doc --locked --all-features --no-deps
npm run docs:check
npm run docs:build
npm run docs:preview
```

Open `http://127.0.0.1:4173/smarter-diagrams/`. The project base path is deliberate: local previews test the same subdirectory hosting used by GitHub Pages. Site checks also use Python 3.11+ and its standard-library TOML parser.

`docs:check` compiles every `diagram` code fence and complete example, typechecks the standalone Rust snippets, and parses TOML/JSON configuration blocks. `docs:build` prepares generated assets, builds a local search index and refuses missing Wasm/API artifacts. `docs:dev` prepares available assets and starts VitePress for editing; build the packages first if you need its demos/API locally.

Architecture, performance and verification pages are generated from their canonical `docs/` files. Edit those originals rather than the generated site copies. Generated SVGs, rustdoc and Wasm bundles are ignored in Git and included in the deployment artifact.

## Browser verification

Install Chromium through Playwright, or set `CHROMIUM_PATH` to an existing executable:

```sh
npx playwright install chromium
DOCS_TEST_URL=http://127.0.0.1:4173/smarter-diagrams/ npm run test:docs
```

The check visits the documentation, uses search, checks internal links/assets, opens the generated API, exercises the embedded/standalone editor, tests examples, exports SVG, runs simulation and changes the spatial controls. It also checks the mobile layout. Screenshots are written under ignored `artifacts/`.

## GitHub Pages deployment

[The Pages workflow](https://github.com/robot-head/smarter-diagrams/blob/main/.github/workflows/pages.yml) builds the native compiler, both Wasm packages, generated Rust API and site. Pull requests build and test without publishing. A successful push to `main` uploads a Pages artifact and deploys it through the `github-pages` environment. A manual workflow dispatch can redeploy `main`.

The repository's Pages source is **GitHub Actions**. Deployment uses the workflow's scoped `pages: write` and `id-token: write` permissions; no personal token is embedded in the site. The published address is [robot-head.github.io/smarter-diagrams/](https://robot-head.github.io/smarter-diagrams/).
