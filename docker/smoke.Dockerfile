# Headless smoke test for filegram.
#
# Builds the release binary, then launches it inside a virtual X display
# with Mesa's software Vulkan (lavapipe) standing in for a GPU. The app
# runs in FILEGRAM_SMOKE mode, which closes the window the instant the
# first frame draws and exits 0. A broken window/wgpu/link path never
# reaches that frame — it panics or aborts, so the container exits
# non-zero and CI goes red.

# ---- build stage ----------------------------------------------------------
FROM rust:1.85-bookworm AS build
WORKDIR /src
# The same system deps the CI Linux build needs (see build.yml).
RUN apt-get update && apt-get install -y --no-install-recommends \
        libxkbcommon-dev libwayland-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release --locked

# ---- runtime stage --------------------------------------------------------
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
        # Virtual display and its auth helper (xvfb-run needs xauth).
        xvfb xauth \
        # Software Vulkan (lavapipe) + the loader wgpu talks to.
        mesa-vulkan-drivers libvulkan1 \
        # winit's X11 backend and keyboard handling.
        libxkbcommon0 libxkbcommon-x11-0 \
        libx11-6 libxcb1 libxcursor1 libxi6 libxrandr2 \
        # winit may probe Wayland even with X11 forced; harmless to have.
        libwayland-client0 \
        # The startup release check speaks HTTPS to GitHub.
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /src/target/release/filegram /usr/local/bin/filegram
COPY docker/smoke-entrypoint.sh /usr/local/bin/smoke-entrypoint.sh
RUN chmod +x /usr/local/bin/smoke-entrypoint.sh

# There is no GPU and no Wayland compositor in here: force lavapipe as the
# only Vulkan device, the Vulkan backend in wgpu, and winit's X11 backend.
ENV VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
    WGPU_BACKEND=vulkan \
    WINIT_UNIX_BACKEND=x11 \
    LIBGL_ALWAYS_SOFTWARE=1 \
    XDG_RUNTIME_DIR=/tmp \
    FILEGRAM_SMOKE=1

ENTRYPOINT ["/usr/local/bin/smoke-entrypoint.sh"]
