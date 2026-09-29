# Etapa 1: compilar el binario "server" en un contenedor con Rust.
# Esto corre en la infraestructura de Fly (o localmente si usás un build
# local), no en tu compu directamente — evita cualquier rareza puntual del
# compilador en tu máquina.
FROM rust:1.90-slim AS builder

WORKDIR /app
COPY . .

RUN cargo build --release --bin server

# Etapa 2: imagen final, liviana, solo con el binario ya compilado.
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/server /usr/local/bin/server

EXPOSE 3000

CMD ["server"]
