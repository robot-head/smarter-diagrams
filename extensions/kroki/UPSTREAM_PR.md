# Upstream `yuzutech/kroki` Integration Plan

Kroki (`https://github.com/yuzutech/kroki`) dispatches diagram formats via `DiagramRegistry` to either local subprocesses, GraalVM/JVM runtimes, or HTTP companion services (`Commander` / `ServiceHealthHandler`).

## Target Upstream Files in `yuzutech/kroki`

1. **`server/src/main/java/io/kroki/server/format/DiagramType.java`**:
   - Add `SMARTERDIAGRAMS("smarterdiagrams", "smarter-diagrams", ...)` alongside existing diagram entries.
2. **`server/src/main/java/io/kroki/server/service/SmarterDiagrams.java`**:
   - Implement `DiagramService` delegating either to the local `smarter-diagrams` binary (`smarter-diagrams - --format <fmt>`) or the companion container over `KROKI_SMARTERDIAGRAMS_HOST` / `KROKI_SMARTERDIAGRAMS_PORT` (`8008`), forwarding supported options (`theme`, `view`, `step`) via `DiagramOptions` / `Kroki-Diagram-Options-*` headers.
3. **`server/src/main/java/io/kroki/server/Server.java`**:
   - Register `/smarterdiagrams` routes (`GET` compressed base64url + `POST` source/JSON) and advertise capabilities cleanly.
4. **Companion container (`Dockerfile` & GitHub workflow)**:
   - Publish a standalone companion image built from `extensions/kroki/Dockerfile` matching Kroki's multi-container release topology.

## Status

Prepared and verified locally via `extensions/kroki/server.cjs` and `npm run test:kroki` (`scripts/kroki-test.cjs`). No automated PR was submitted to `yuzutech/kroki` from this workspace so an upstream maintainer review can be coordinated manually with published release artifacts.
