# rumdl container image: a single static musl binary, in two flavours.
#
# - scratch (default): nothing but the binary. Tags :<version> and :latest.
#   ENTRYPOINT is rumdl itself; run as `docker run ... <image> check .`.
# - alpine: the same binary on an Alpine base, for environments that need a
#   shell inside the image (e.g. GitLab CI job containers, which run their
#   own setup scripts before the job). Tags :<version>-alpine and :alpine.
#   No ENTRYPOINT: the default shell starts and rumdl is on PATH.
#
# The build context is a staging directory containing prebuilt binaries at
# binaries/<arch>/rumdl (amd64, arm64). CI populates it from release
# artifacts; `make docker-binaries` populates it from local cargo-zigbuild
# builds. Build via `make docker-build` / `make docker-verify`; select a
# flavour with --target <flavour>-image. The scratch stage is last so a
# plain `docker build` without --target still produces the scratch image.
ARG ALPINE_VERSION=3.24

FROM alpine:${ALPINE_VERSION} AS alpine-image

ARG TARGETARCH
ARG VERSION=dev
ARG REVISION=unknown

LABEL org.opencontainers.image.title="rumdl" \
      org.opencontainers.image.description="A high-performance Markdown linter and formatter, written in Rust" \
      org.opencontainers.image.source="https://github.com/rvben/rumdl" \
      org.opencontainers.image.url="https://github.com/rvben/rumdl" \
      org.opencontainers.image.documentation="https://github.com/rvben/rumdl#readme" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}"

COPY binaries/${TARGETARCH}/rumdl /usr/local/bin/rumdl

WORKDIR /data

# Runs as root with the Alpine default shell entrypoint: CI runners execute
# their own setup inside the job container and need a writable filesystem
# and a shell. Invoke the linter as `rumdl ...`.
CMD ["rumdl", "--help"]

FROM scratch AS scratch-image

ARG TARGETARCH
ARG VERSION=dev
ARG REVISION=unknown

LABEL org.opencontainers.image.title="rumdl" \
      org.opencontainers.image.description="A high-performance Markdown linter and formatter, written in Rust" \
      org.opencontainers.image.source="https://github.com/rvben/rumdl" \
      org.opencontainers.image.url="https://github.com/rvben/rumdl" \
      org.opencontainers.image.documentation="https://github.com/rvben/rumdl#readme" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}"

COPY binaries/${TARGETARCH}/rumdl /usr/local/bin/rumdl

# Mount the project to lint at /data: docker run -v "$PWD:/data" <image> check .
WORKDIR /data

# Run as the conventional distroless nonroot uid:gid by default so the
# container never writes root-owned files into a bind-mounted project. The
# lint cache is skipped gracefully when /data is not writable for this user;
# pass --user "$(id -u):$(id -g)" to enable it with your own ownership.
USER 65532:65532

ENTRYPOINT ["/usr/local/bin/rumdl"]
CMD ["--help"]
