# --- Compilation ---
FROM rust:1.90-slim-bookworm AS build
WORKDIR /app
# Les requêtes SQL sont vérifiées hors ligne grâce au dossier .sqlx (cargo sqlx prepare).
ENV SQLX_OFFLINE=true
COPY Cargo.toml Cargo.lock ./
COPY .sqlx .sqlx
COPY migrations migrations
COPY src src
RUN cargo build --release --locked

# --- Image finale ---
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 volta
COPY --from=build /app/target/release/volta-api /usr/local/bin/volta-api
USER volta
EXPOSE 8080
CMD ["volta-api"]
