FROM rust:1.100-slim-bookworm@sha256:7c7d4eb4d05b7781187a327809466cfddf29753f6defdc3d19a0e291951b7a37 AS builder

WORKDIR /src
COPY . .
RUN cargo build --release --locked --features export --bin smarter-diagrams

FROM node:26-bookworm-slim@sha256:5df79989ed685e09f0a98019db4cebfb22fb9df1a53c0161664a011f7ff6b0f1

RUN useradd --create-home --shell /bin/false kroki
COPY --from=builder /src/target/release/smarter-diagrams /usr/local/bin/smarter-diagrams
COPY extensions/kroki/server.cjs /opt/smarter-diagrams/server.cjs

ENV SMARTER_DIAGRAMS_BIN=/usr/local/bin/smarter-diagrams
ENV KROKI_HOST=0.0.0.0
ENV KROKI_PORT=8008

USER kroki
EXPOSE 8008
HEALTHCHECK --interval=30s --timeout=5s CMD node -e "fetch('http://127.0.0.1:8008/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"
CMD ["node", "/opt/smarter-diagrams/server.cjs"]
