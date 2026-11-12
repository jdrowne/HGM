FROM debian:stable-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl gcc libc6-dev git patch musl-tools && rm -rf /var/lib/apt/lists/*

ENV PATH=/root/.cargo/bin:$PATH CARGO_PROFILE_DEV_DEBUG=line-tables-only

WORKDIR /rustyhgm

COPY rust-toolchain.toml .

RUN curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o /tmp/rustup.sh \
    && sh /tmp/rustup.sh -y --default-toolchain none --profile minimal \
    && rm /tmp/rustup.sh \
    && rustup toolchain install \
    && rustup target add "$(uname -m)-unknown-linux-musl"

COPY Cargo.toml .
COPY src/coding-harness/Cargo.toml src/coding-harness/

RUN mkdir -p test/src/coding-harness/unit test/src/coding-harness/integration \
    && touch src/coding-harness/lib.rs test/src/coding-harness/unit/main.rs test/src/coding-harness/integration/main.rs \
    && printf 'fn main() {}\n' > src/coding-harness/main.rs \
    && cargo build --release --target "$(uname -m)-unknown-linux-musl" -p rustyhgm-coding-harness \
    && cargo test -p rustyhgm-coding-harness --no-run

COPY . .

RUN printf 'target/\nCargo.lock\n' > .gitignore \
    && find src test -type f -exec touch {} + \
    && cargo build --release --target "$(uname -m)-unknown-linux-musl" -p rustyhgm-coding-harness \
    && cargo test -p rustyhgm-coding-harness --no-run \
    && "target/$(uname -m)-unknown-linux-musl/release/rustyhgm-coding-harness" --help | grep -q Usage

CMD ["tail", "-f", "/dev/null"]
