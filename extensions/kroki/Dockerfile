FROM rust:1.100-slim-bookworm@sha256:bb875b0f8ce726053e259034848c471840e96bfe591fd52556178ae9a05e6126 AS builder

WORKDIR /src
COPY . .
RUN cargo build --release --locked --features export --bin smarter-diagrams

FROM node:26-bookworm-slim@sha256:0042e17d232a87a2cd485be1da7900fca4d10a4ad11b29e9285da1d2e54798c9

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
