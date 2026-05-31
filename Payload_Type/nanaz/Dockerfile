FROM itsafeaturemythic/mythic_python_base:latest

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
RUN chmod -R a+w $RUSTUP_HOME $CARGO_HOME

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        mingw-w64 \
        musl-tools \
    && rm -rf /var/lib/apt/lists/*

RUN python -m pip install cryptography

WORKDIR /Mythic/
COPY [".", "."]

RUN cd nanaz/agent_code && \
    . "$CARGO_HOME/env" && \
    rustup target add x86_64-pc-windows-gnu && \
    rustup target add x86_64-unknown-linux-gnu && \
    rustup target add x86_64-unknown-linux-musl && \
    cargo fetch && \
    cargo build && \
    cargo build -r

CMD ["python3", "main.py"]
