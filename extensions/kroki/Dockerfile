FROM rust:1.86-slim-bookworm@sha256:57d415bbd61ce11e2d5f73de068103c7bd9f3188dc132c97cef4a8f62989e944 AS builder

WORKDIR /src
COPY . .
RUN cargo build --release --locked --features export --bin smarter-diagrams

FROM node:22-bookworm-slim@sha256:e9f565d32859f0cbfb52125eafe8be39390f24dac302dc3cdf566e8ba3ab21ee

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
