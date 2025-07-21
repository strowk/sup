FROM debian:12-slim AS builder

RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && curl -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly  -y

ENV PATH="/root/.cargo/bin:${PATH}"
RUN apt-get update && apt-get install -y \
    build-essential \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-deb

RUN apt-get update && apt-get install -y \
    debsigs \
    gnupg \
    reprepro \
    && rm -rf /var/lib/apt/lists/*


ARG VERSION=SNAPSHOT
COPY Cargo.toml Cargo.lock /app/
COPY src /app/src
COPY target/x86_64-unknown-linux-gnu/ /app/target/x86_64-unknown-linux-gnu/
WORKDIR /app
RUN cargo deb --no-build --target=x86_64-unknown-linux-gnu
RUN dpkg -i target/x86_64-unknown-linux-gnu/debian/git-sup_${VERSION}-1_amd64.deb
RUN sup --version

