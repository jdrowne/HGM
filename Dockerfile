FROM debian:stable-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl gcc libc6-dev git patch musl-tools && rm -rf /var/lib/apt/lists/*

ENV PATH=/root/.cargo/bin:$PATH CARGO_PROFILE_DEV_DEBUG=line-tables-only

RUN curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o /tmp/rustup.sh && sh /tmp/rustup.sh -y --default-toolchain none --profile minimal && rm /tmp/rustup.sh

WORKDIR /rustyhgm

COPY . .

RUN printf 'target/\nCargo.lock\n' > .gitignore \
    && rustup toolchain install \
    && rustup target add "$(uname -m)-unknown-linux-musl" \
    && cargo build --release --target "$(uname -m)-unknown-linux-musl" -p rustyhgm-coding-harness \
    && cargo test -p rustyhgm-coding-harness --no-run

CMD ["tail", "-f", "/dev/null"]
