# rumdl container image: a single static musl binary on a scratch base.
#
# The build context is a staging directory containing prebuilt binaries at
# binaries/<arch>/rumdl (amd64, arm64). CI populates it from release
# artifacts; `make docker-binaries` populates it from local cargo-zigbuild
# builds. Build via `make docker-build` / `make docker-verify`.
FROM scratch

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
