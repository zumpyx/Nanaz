FROM itsafeaturemythic/mythic_python_base:latest

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
RUN curl https://ziglang.org/download/0.16.0/zig-x86_64-linux-0.16.0.tar.xz -o /tmp/zig.tar.xz && \
    tar -xvf /tmp/zig.tar.xz -C /tmp && \
    mv /tmp/zig-x86_64-linux-0.16.0 /usr/local/zig && \
    ln -s /usr/local/zig/zig /usr/local/cargo/bin/zig

RUN chmod -R a+w $RUSTUP_HOME $CARGO_HOME

RUN python -m pip install cryptography

WORKDIR /Mythic/
COPY [".", "."]

RUN cd nanaz/agent_code && \
    . "$CARGO_HOME/env" && \
    rustup target add x86_64-pc-windows-gnu && \
    rustup target add x86_64-unknown-linux-gnu && \
    rustup target add x86_64-unknown-linux-musl && \
    cargo install cargo-zigbuild && \
    cargo fetch && \
    cargo build && \
    cargo build -r

CMD ["python3", "main.py"]
