FROM itsafeaturemythic/mythic_python_base:latest

# Pinned toolchain versions. Bump deliberately; lockfile v4 needs Rust 1.78+.
ARG RUST_VERSION=1.85.0
ARG ZIG_VERSION=0.16.0
ARG CARGO_ZIGBUILD_VERSION=0.19.10

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# Ensure ca-certificates present (used by curl + rustup).
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --default-toolchain ${RUST_VERSION}
RUN curl https://ziglang.org/download/${ZIG_VERSION}/zig-x86_64-linux-${ZIG_VERSION}.tar.xz -o /tmp/zig.tar.xz && \
    tar -xvf /tmp/zig.tar.xz -C /tmp && \
    mv /tmp/zig-x86_64-linux-${ZIG_VERSION} /usr/local/zig && \
    ln -s /usr/local/zig/zig /usr/local/cargo/bin/zig

RUN chmod -R a+w $RUSTUP_HOME $CARGO_HOME

RUN python -m pip install cryptography

WORKDIR /Mythic/
COPY [".", "."]

# Build every (target, profile) combination so Mythic can hand out the right
# binary regardless of what the operator picks. Zigbuild links with the zig
# toolchain for both targets — no separate GNU/mingw toolchain needed.
#
#   target                                 debug   release
#   x86_64-unknown-linux-musl   (Linux)    [built]  [built]
#   x86_64-pc-windows-gnu       (Windows)  [built]  [built]
RUN cd nanaz/agent_code && \
    . "$CARGO_HOME/env" && \
    cargo install cargo-zigbuild --version ${CARGO_ZIGBUILD_VERSION} --locked && \
    cargo fetch && \
    # Provide a placeholder config.json so the embedded JSON loader doesn't
    # fall back to all-zeros. The Mythic builder overwrites this with the
    # real PSK + UUID at payload-build time.
    cp -n config.example.json config.json || true && \
    for target in x86_64-unknown-linux-musl x86_64-pc-windows-gnu; do \
        for profile in "" --release; do \
            echo ">>> cargo zigbuild --target $target $profile"; \
            cargo zigbuild --target "$target" $profile; \
        done; \
    done

CMD ["python3", "main.py"]
