# Upstream `yuzutech/kroki` Integration

Kroki (`https://github.com/yuzutech/kroki`) integrates pure-Rust single-binary compilers (such as `svgbob` and `lini`) directly into the Vert.x gateway server via `io.kroki.server.action.Commander`, piping diagram source over `stdin` and reading rendered bytes from `stdout`.

- **Upstream PR**: https://github.com/yuzutech/kroki/pull/2297
- **Fork branch**: https://github.com/robot-head/kroki/tree/feat/smarterdiagrams
- **Pinned release tag**: https://github.com/robot-head/smarter-diagrams/releases/tag/v0.1.0
- **Patch file**: [`yuzutech-kroki-smarterdiagrams.patch`](./yuzutech-kroki-smarterdiagrams.patch) (`git format-patch`)

## Upstream Files in `yuzutech/kroki`

1. **Java Gateway Service & Tests**:
   - `server/src/main/java/io/kroki/server/service/SmarterDiagrams.java` — implements `DiagramService` supporting `SVG`, `PNG`, and `PDF`, delegating to `smarter-diagrams - --format <fmt>` configured via `KROKI_SMARTERDIAGRAMS_BIN_PATH`, and declaring `DiagramOption` descriptors for `theme` (restricted to the 16 built-in themes), `view`, and `step` (strict identifier validation; `view` and `step` cannot be combined; `--theme-file` and `-o` are never exposed so zero local files are read or written under every `KROKI_SAFE_MODE`).
   - `server/src/main/java/io/kroki/server/Server.java` — registers `"smarterdiagrams"` in `registerDiagramServices(...)` (wiring `GET /smarterdiagrams/:format/:encoded`, `POST /smarterdiagrams/:format`, `POST /`, `/health`, and `/openapi.json`).
   - `server/src/main/resources/web/hello.html` — adds Smarter Diagrams to the landing list.
   - `server/src/test/java/io/kroki/server/service/SmarterDiagramsServiceTest.java` — JUnit 5 unit tests covering `svg`/`png`/`pdf` CLI arguments, option forwarding, invalid option rejection (`400 Bad Request`), and unsupported format handling.
2. **Docker Build & CI Tooling**:
   - `server/ops/docker/Dockerfile` — adds the `kroki-builder-static-smarterdiagrams` stage (`rust:1.98-slim-bullseye`) cross-compiling `smarter-diagrams` from tag `v0.1.0` with `--locked --features export` for `amd64`/`arm64`, copies `/usr/bin/smarter-diagrams` into the runtime image, and sets `ENV KROKI_SMARTERDIAGRAMS_BIN_PATH=/usr/bin/smarter-diagrams`.
   - `ci/tasks/update-versions.js` — tracks `smarterdiagrams` from `SMARTERDIAGRAMS_VERSION` and syncs `SmarterDiagrams.java` and `docs/antora.yml`.
   - `ci/tests/diagrams/services.diagram` & `ci/tests/smoke.js` — end-to-end smoke coverage for `svg`, `png`, `pdf`, `theme`, `view`, and `step`.
3. **Documentation & Metadata**:
   - `CHANGELOG.md`, `DOCKERHUB.md`, `README.adoc`, `docs/antora.yml`, `docs/modules/ROOT/pages/{architecture,diagram-types,index}.adoc`, `docs/modules/setup/pages/{configuration,diagram-options,install}.adoc`.

## Verification

- `./mvnw --no-transfer-progress package` on JDK 21: `327` tests run, `0` failures, `0` errors (`70` pre-existing native-binary integration tests skipped on Windows), including `SmarterDiagramsServiceTest` (`8/8`), `DiagramOptionMetadataTest`, and `OpenApiTest` (`17/17`).
- Live HTTP integration check against `kroki-server.jar` with `KROKI_SMARTERDIAGRAMS_BIN_PATH` pointing to `smarter-diagrams` `v0.1.0`: verified `/health`, `/openapi.json`, `POST /smarterdiagrams/{svg,png,pdf}`, `POST /` JSON options (`theme`, `step`, `view`), `GET /smarterdiagrams/svg/{encoded}`, and HTTP `400` option rejection.
- `node ci/tasks/update-versions.js` synced `smarterdiagrams: '0.1.0'` (`count: 31`) cleanly, and `git apply --check` confirmed the exported patch applies cleanly to `yuzutech/kroki` `main`.
