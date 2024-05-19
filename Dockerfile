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




FROM ubuntu:noble
RUN apt update && apt install -y texlive-full && rm -rf /var/lib/apt
RUN apt update && apt install -y sqlite3 make python3 python-is-python3 python3-pip python3-psutil jq curl wget python3-pandocfilters && rm -rf /var/lib/apt
RUN apt update && cd /tmp && wget https://github.com/jgm/pandoc/releases/download/3.2/pandoc-3.2-1-amd64.deb && apt install -y ./pandoc-3.2-1-amd64.deb && rm -rf /tmp/* && rm -rf /var/lib/apt
RUN cd /tmp && wget https://github.com/lierdakil/pandoc-crossref/releases/download/v0.3.17.1a/pandoc-crossref-Linux.tar.xz && tar -xf pandoc-crossref-Linux.tar.xz && cp pandoc-crossref /usr/local/bin/pandoc-crossref && rm -rf /tmp/*
RUN cd /usr/local/share/fonts && curl https://fonts.google.com/download/list?family=PT%20Mono,PT%20Sans,PT%20Serif | tail -n +2 | jq ".manifest.fileRefs[].url" -r | xargs -I{} wget {}
RUN fc-cache -f -v
COPY --from=builder /exec /exec
ENTRYPOINT ["/exec"]
