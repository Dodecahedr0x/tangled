# Hosted knot and its loopback native pairing broker share one persistent host.
FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY apps apps
RUN cargo build --release -p tangled-provider

FROM node:22-bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates python3 python3-venv procps tini \
    && rm -rf /var/lib/apt/lists/* \
    && python3 -m venv /opt/venv
WORKDIR /opt/tangled
COPY connectors/requirements.lock connectors/requirements.lock
RUN /opt/venv/bin/pip install --no-cache-dir -r connectors/requirements.lock
COPY connectors/whatsapp/package.json connectors/whatsapp/package-lock.json connectors/whatsapp/
RUN npm ci --prefix connectors/whatsapp --omit=dev --ignore-scripts \
    && npm cache clean --force
COPY connectors connectors
COPY --from=build /src/target/release/tangled-provider /usr/local/bin/tangled-provider
ENV PORT=10000 \
    TANGLED_HOSTED_PAIRING=1 \
    TANGLED_PAIRING_PORT=8792 \
    PYTHONUNBUFFERED=1
EXPOSE 10000
ENTRYPOINT ["/usr/bin/tini", "--", "/opt/venv/bin/python", "/opt/tangled/connectors/hosted/supervise.py"]
CMD ["/usr/local/bin/tangled-provider"]
