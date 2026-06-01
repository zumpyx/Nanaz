FROM alpine

ARG ZIG=zig-x86_64-linux-0.16.0
ARG RUST=rust-1.96.0-x86_64-unknown-linux-musl
ARG RUST_TARGET=x86_64-unknown-linux-musl

RUN apk update && \
    apk add --no-cache python3 && \
    apk add --no-cache --virtual .build-deps py3-pip gcc python3-dev musl-dev linux-headers tar xz && \
    pip install --no-cache-dir --root-user-action ignore --break-system-packages mythic-container pycryptodome cryptography

RUN wget https://ziglang.org/download/0.16.0/${ZIG}.tar.xz && \
    tar -xvf ${ZIG}.tar.xz -C /usr/local/share && \
    ln -s /usr/local/share/${ZIG}/zig /usr/local/bin/zig && \
    rm ${ZIG}.tar.xz

RUN wget https://static.rust-lang.org/dist/${RUST}.tar.xz && \
    tar -xvf ${RUST}.tar.xz -C /usr/local/share && \
    rm ${RUST}.tar.xz && \
    ln -s /usr/local/share/${RUST}/cargo/bin/* /usr/local/bin/ && \
    ln -s /usr/local/share/${RUST}/rustc/bin/* /usr/local/bin/ && \
    ln -s /usr/local/share/${RUST}/clippy-preview/bin/* /usr/local/bin/ && \
    ln -s /usr/local/share/${RUST}/rustfmt-preview/bin/* /usr/local/bin/ && \
    ln -s /usr/local/share/${RUST}/rust-analyzer-preview/bin/* /usr/local/bin/ && \
    ln -s /usr/local/share/${RUST}/rust-std-${RUST_TARGET}/lib/rustlib/${RUST_TARGET}/lib \
        /usr/local/share/${RUST}/rustc/lib/rustlib/${RUST_TARGET}/lib
    cargo install cargo-zigbuild --root /usr/local --no-track && \
    rm -rf /root/.cargo/registry && \
    rm -rf /root/.cargo/git && \
    rm -rf /tmp/cargo-install*

RUN apk del .build-deps

WORKDIR /Mythic/
COPY [".", "."]

ARG TARGETS="x86_64-unknown-linux-musl x86_64-pc-windows-gnu"
ARG AGENT_PATH=nanaz/agent_code

RUN cd ${AGENT_PATH} && \
    cp -n config.example.json config.json && cargo fetch && \
    for TARGET in ${TARGETS}; do \
        cargo zigbuild --release --target $TARGET && \
        cargo zigbuild --debug --target $TARGET; \
    done

CMD ["python3", "main.py"]
