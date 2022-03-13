FROM rust:1.59-slim-buster as builder

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata pkg-config libssl-dev cmake autoconf libopus-dev libtool
RUN mkdir -p /code
WORKDIR /code
RUN USER=root cargo init --bin disco-music-bot
COPY ./Cargo.toml /code/disco-music-bot/Cargo.toml
WORKDIR /code/disco-music-bot
RUN cargo build --release
RUN rm src/*.rs

ADD . .

WORKDIR /code/disco-music-bot
RUN cargo build --release

