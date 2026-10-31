FROM debian:12-slim AS builder

RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && curl -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly  -y

ENV PATH="/root/.cargo/bin:${PATH}"
# build-essential/libssl-dev/pkg-config build the crate; perl + cmake are
# required by the vendored openssl and libgit2-sys builds; git is needed for
# the git dependencies in Cargo.toml.
RUN apt-get update && apt-get install -y \
    build-essential \
    libssl-dev \
    pkg-config \
    perl \
    cmake \
    git \
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
WORKDIR /app
# Build inside debian:12 so the binary links against this base's glibc (2.36),
# keeping the package installable on Debian 12 and newer. Building against a
# newer glibc (as the cross-produced release binaries do) breaks `sup --version`
# here and in that older environment.
RUN cargo build --release --target x86_64-unknown-linux-gnu
RUN cd /app/target/x86_64-unknown-linux-gnu/release && \
    ln -sf sup git-sup
RUN cargo deb --no-build --target=x86_64-unknown-linux-gnu
RUN dpkg -i target/x86_64-unknown-linux-gnu/debian/git-sup_${VERSION}-1_amd64.deb
RUN sup --version

