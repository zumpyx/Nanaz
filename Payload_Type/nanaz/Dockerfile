FROM itsafeaturemythic/mythic_python_base:latest

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN python -m pip install cryptography

WORKDIR /Mythic/
COPY [".", "."]

RUN cd nanaz/agent_code && . "$HOME/.cargo/env" && cargo fetch

CMD ["bash", "-c", "/usr/bin/env python main.py"]
