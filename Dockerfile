FROM rust:1.88-trixie AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

# Build release binary
RUN cargo build --release

FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libstdc++6 && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/openwhisper-cleanup-server /usr/local/bin/
COPY models/ /app/models/

WORKDIR /app
EXPOSE 8787

ENTRYPOINT ["openwhisper-cleanup-server"]
CMD ["--model-path", "models/pcs_47lang.onnx", "--tokenizer-path", "models/tokenizer.json", "--dict-dir", "models/dictionaries"]
