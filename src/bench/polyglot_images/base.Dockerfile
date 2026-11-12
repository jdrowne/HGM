FROM buildpack-deps:jammy

RUN apt-get update && apt-get install -y \
    software-properties-common \
    cmake \
    libboost-all-dev \
    && add-apt-repository ppa:deadsnakes/ppa \
    && apt-get update \
    && apt-get install -y \
    python3.11 \
    python3.11-venv \
    python3.11-dev \
    python3-pip \
    ca-certificates-java \
    openjdk-21-jdk \
    libtbb-dev \
    && rm -rf /var/lib/apt/lists/*

RUN ARCH=$(uname -m) && \
    if [ "$ARCH" = "x86_64" ]; then \
        GOARCH="amd64"; \
    elif [ "$ARCH" = "aarch64" ]; then \
        GOARCH="arm64"; \
    else \
        false; \
    fi && \
    curl -L "https://golang.org/dl/go1.21.5.linux-$GOARCH.tar.gz" -o go.tar.gz && \
    tar -C /usr/local -xzf go.tar.gz && \
    rm go.tar.gz
ENV PATH="/usr/local/go/bin:${PATH}"

ADD https://sh.rustup.rs /tmp/rustup.sh
RUN chmod +x /tmp/rustup.sh \
    && /tmp/rustup.sh -y --profile minimal --component rustfmt,clippy \
    && rm /tmp/rustup.sh
ENV PATH="/root/.cargo/bin:${PATH}"

RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/* && \
    mkdir -p /npm-install && \
    cd /npm-install && \
    npm init -y && \
    npm install \
    jest \
    @babel/core@7.25.2 \
    @exercism/babel-preset-javascript@0.2.1 \
    @exercism/eslint-config-javascript@0.6.0 \
    @types/jest@29.5.12 \
    @types/node@20.12.12 \
    babel-jest@29.6.4 \
    core-js@3.37.1 \
    eslint@8.49.0 && \
    npm cache clean --force

RUN wget "https://repo.anaconda.com/miniconda/Miniconda3-py311_23.11.0-2-Linux-$(uname -m).sh" -O miniconda.sh \
    && bash miniconda.sh -b -p /opt/miniconda3 \
    && rm miniconda.sh \
    && /opt/miniconda3/bin/conda clean -afy
ENV PATH=/opt/miniconda3/bin:$PATH
RUN conda init --all
RUN conda config --append channels conda-forge

RUN adduser --disabled-password --gecos 'dog' nonroot
