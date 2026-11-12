FROM debian:stable-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    gcc \
    g++ \
    libc6-dev \
    make \
    cmake \
    libboost-date-time-dev \
    libtbb-dev \
    openjdk-21-jdk-headless \
    && rm -rf /var/lib/apt/lists/*

RUN ARCH=$(uname -m) && \
    if [ "$ARCH" = "x86_64" ]; then \
        GOARCH="amd64"; \
        GOSUM="e2bc0b3e4b64111ec117295c088bde5f00eeed1567999ff77bc859d7df70078e"; \
    elif [ "$ARCH" = "aarch64" ]; then \
        GOARCH="arm64"; \
        GOSUM="841cced7ecda9b2014f139f5bab5ae31785f35399f236b8b3e75dff2a2978d96"; \
    else \
        false; \
    fi && \
    curl -fsSL "https://go.dev/dl/go1.21.5.linux-$GOARCH.tar.gz" -o go.tar.gz && \
    echo "$GOSUM  go.tar.gz" | sha256sum -c - && \
    tar -C /usr/local -xzf go.tar.gz && \
    rm go.tar.gz
ENV PATH="/usr/local/go/bin:${PATH}"

RUN curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o /tmp/rustup.sh \
    && sh /tmp/rustup.sh -y --profile minimal --component rustfmt,clippy \
    && rm /tmp/rustup.sh
ENV PATH="/root/.cargo/bin:${PATH}"

RUN curl -fsSL https://deb.nodesource.com/setup_20.x -o /tmp/nodesource.sh && \
    bash /tmp/nodesource.sh && \
    rm /tmp/nodesource.sh && \
    apt-get install -y --no-install-recommends nodejs && \
    rm -rf /var/lib/apt/lists/* && \
    dpkg-query -W -f '${Version}' nodejs | grep -q nodesource && \
    mkdir -p /npm-install && \
    cd /npm-install && \
    npm init -y && \
    npm install \
    jest@29.7.0 \
    @babel/core@7.25.2 \
    @exercism/babel-preset-javascript@0.2.1 \
    @exercism/eslint-config-javascript@0.6.0 \
    @types/jest@29.5.12 \
    @types/node@20.12.12 \
    babel-jest@29.6.4 \
    core-js@3.37.1 \
    eslint@8.49.0 && \
    npm cache clean --force

RUN RELEASE=$(curl -fsSL -o /dev/null -w '%{url_effective}' https://github.com/conda-forge/miniforge/releases/latest) \
    && VERSION=${RELEASE##*/} \
    && FILE="Miniforge3-$VERSION-Linux-$(uname -m).sh" \
    && cd /tmp \
    && curl -fsSL "https://github.com/conda-forge/miniforge/releases/download/$VERSION/$FILE" -o "$FILE" \
    && curl -fsSL "https://github.com/conda-forge/miniforge/releases/download/$VERSION/$FILE.sha256" | sha256sum -c - \
    && bash "$FILE" -b -p /opt/miniconda3 \
    && rm "$FILE" \
    && /opt/miniconda3/bin/conda clean -afy
ENV PATH=/opt/miniconda3/bin:$PATH
