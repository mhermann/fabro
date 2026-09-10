# Build image for the Fabro sandbox agent (lib/apps/fabro-sandbox-agent).
#
# The agent ships as a static musl binary in a distroless image so it can run
# as the init process of any user sandbox image. Binaries are supplied
# pre-built via `cargo dev agent-image` (or the release workflow):
#   tmp/docker-context/amd64/fabro-sandbox-agent  (x86_64-unknown-linux-musl)
#   tmp/docker-context/arm64/fabro-sandbox-agent  (aarch64-unknown-linux-musl)
#
# Build with `cargo dev agent-image` (preferred) or:
#   docker build -f lib/apps/fabro-sandbox-agent/Dockerfile \
#       --build-arg TARGETARCH=amd64 -t fabro-sandbox-agent .

FROM gcr.io/distroless/static-debian12

ARG TARGETARCH

# The image carries exactly one file: the pod's init container copies it into
# the shared `emptyDir` and the main container runs it as PID 1. There is no
# entrypoint; the pod manifest supplies the command.
COPY --chmod=0755 tmp/docker-context/${TARGETARCH}/fabro-sandbox-agent /fabro-sandbox-agent
