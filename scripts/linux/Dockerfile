# Ambiente di build Linux come il job della CI (ubuntu-22.04): .deb, AppImage, swapdeck-cli e .vsix linux-x64.
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:$PATH \
    APPIMAGE_EXTRACT_AND_RUN=1

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git build-essential pkg-config \
      libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev \
      patchelf file libfuse2 xdg-utils \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g pnpm@10.28.1 \
    && curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
    && git config --global --add safe.directory '*'

COPY build.sh /usr/local/bin/swapdeck-build
RUN chmod +x /usr/local/bin/swapdeck-build
ENTRYPOINT ["swapdeck-build"]
