FROM rust:1.92.0-alpine
WORKDIR /app
RUN apk update && apk add lld clang
ENV SQLX_OFFLINE true
COPY . .
RUN cargo build --release
ENTRYPOINT ["./target/release/zero2prod"]