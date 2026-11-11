FROM rust:latest

RUN apt-get update && apt-get install -y musl-tools patch && apt-get clean && rm -rf /var/lib/apt/lists/*

WORKDIR /rustyhgm

COPY . .

RUN printf 'target/\nCargo.lock\n' > .gitignore \
    && rustup target add "$(uname -m)-unknown-linux-musl" \
    && cargo build --release --target "$(uname -m)-unknown-linux-musl" -p rustyhgm-coding-harness \
    && cargo test -p rustyhgm-coding-harness --no-run

CMD ["tail", "-f", "/dev/null"]
