FROM rust:1.85.0-bullseye as builder
WORKDIR /usr/src/duplikate
COPY . .
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN cargo install --path .

FROM debian:bullseye-slim
COPY --from=builder /usr/local/cargo/bin/duplikate /usr/local/bin/duplikate
CMD ["duplikate"]