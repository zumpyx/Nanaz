FROM itsafeaturemythic/mythic_python_base:latest

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN python -m pip install cryptography

WORKDIR /Mythic/
COPY [".", "."]

RUN cd nanaz/agent_code && \
    . "$HOME/.cargo/env" && \
    rustup target add x86_64-pc-windows-gnu && \
    rustup target add x86_64-unknown-linux-gnu && \
    rustup target add x86_64-unknown-linux-musl && \
    cargo fetch

CMD ["python3", "main.py"]
