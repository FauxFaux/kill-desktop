FROM buildpack-deps:14.04-curl

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y

RUN export DEBIAN_FRONTEND=noninteractive && \
    apt-get update && \
    apt-get upgrade -y && \
    apt-get install -y libxcb1-dev build-essential && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

