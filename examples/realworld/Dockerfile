# Stage 1: Build the Elm frontend
FROM node:20-slim AS elm-build
RUN npm install -g elm
WORKDIR /app/frontend
COPY frontend/elm.json .
COPY frontend/src/ src/
RUN elm make src/Main.elm --optimize --output=public/elm.js
COPY frontend/public/index.html public/

# Stage 2: Build the Rust backend
FROM rust:1.83-slim AS rust-build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY ../../Cargo.toml ../../Cargo.lock ./
COPY ../../typeway-core typeway-core/
COPY ../../typeway-server typeway-server/
COPY ../../typeway-client typeway-client/
COPY ../../typeway-openapi typeway-openapi/
COPY ../../typeway-macros typeway-macros/
COPY ../../typeway typeway/
COPY ../../spike spike/
COPY ../../examples/realworld examples/realworld/
RUN cargo build -p typeway-realworld --release

# Stage 3: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust-build /app/target/release/typeway-realworld .
COPY --from=elm-build /app/frontend/public/ frontend/public/

ENV FRONTEND_DIR=/app/frontend/public
ENV DATABASE_HOST=db
ENV DATABASE_PORT=5432
ENV DATABASE_NAME=realworld
ENV DATABASE_USER=postgres
ENV DATABASE_PASSWORD=postgres

EXPOSE 4000
CMD ["./typeway-realworld"]
