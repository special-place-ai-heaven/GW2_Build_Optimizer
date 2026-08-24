# syntax=docker/dockerfile:1
FROM rust:1-bookworm AS build
WORKDIR /src
COPY server/feedback/Cargo.toml server/feedback/Cargo.lock* ./server/feedback/
COPY data/feedback_taxonomy.json ./data/feedback_taxonomy.json
# Prime the dependency cache with a stub main so source edits don't rebuild deps.
RUN mkdir -p server/feedback/src && echo 'fn main(){}' > server/feedback/src/main.rs && echo '' > server/feedback/src/lib.rs \
 && cd server/feedback && cargo build --release && rm -rf src
COPY server/feedback ./server/feedback
# BuildKit's COPY preserves host mtimes, which can be older than the stub's
# cargo fingerprint above and fool the mtime-based rebuild check into
# skipping recompilation (shipping the stub). Touch sources to force a real build.
RUN find server/feedback/src -name '*.rs' -exec touch {} + \
 && cd server/feedback && cargo build --release

FROM debian:bookworm-slim
# wget is here for the compose healthcheck; bookworm-slim ships neither it nor curl.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates wget && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/server/feedback/target/release/gw2bo-feedback /usr/local/bin/gw2bo-feedback
USER 65534:65534
EXPOSE 8080
ENTRYPOINT ["gw2bo-feedback"]
