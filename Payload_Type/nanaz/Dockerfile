FROM messense/cargo-zigbuild@sha256:6eb3714fd0153e9407cef350975e34cd635de5d1e9f5c319428bc732182b99b5

RUN rustup target add x86_64-unknown-linux-musl x86_64-pc-windows-gnu

RUN apt-get update && apt-get install -y --no-install-recommends python3-pip python3-dev

RUN python3 -m pip install --no-cache-dir --break-system-packages mythic-container pycryptodome cryptography

RUN apt-get purge -y --auto-remove python3-pip python3-dev && apt-get clean && rm -rf /var/lib/apt/lists/*

WORKDIR /Mythic/
COPY [".", "."]

RUN cd nanaz/agent_code && cargo fetch

CMD ["python3", "main.py"]
