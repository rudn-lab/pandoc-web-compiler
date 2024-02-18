FROM rust:buster AS builder

RUN cargo install cargo-strip

COPY . /app
WORKDIR /app
RUN --mount=type=cache,target=/usr/local/cargo/git,id=${TARGETARCH} \
    --mount=type=cache,target=/usr/local/cargo/registry,id=${TARGETARCH} \
    --mount=type=cache,target=/app/target,id=${TARGETARCH} \
    echo "Current compilation cache size:" && \
    du -csh /app/target /usr/local/cargo/registry /usr/local/cargo/git && \
    cargo build --release && \
    cargo strip && \
    # Copy executable out of the cache so it is available in the final image.
    cp target/release/rudn-yamadharma-course-compiler /exec




FROM pandoc/extra:edge-ubuntu
RUN apt update && apt install -y sqlite3 make && rm -rf /var/lib/apt
COPY --from=builder /exec /exec
ENTRYPOINT ["/exec"]
