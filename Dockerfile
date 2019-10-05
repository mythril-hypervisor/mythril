FROM archlinux/base
MAINTAINER Adam Schwalm <adamschwalm@gmail.com>

RUN pacman -y --noconfirm -S rustup gcc git sudo binutils make fakeroot

RUN useradd build && mkdir /home/build && chown build:build /home/build
RUN echo "build ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers
RUN echo "root ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers

RUN git clone https://aur.archlinux.org/yay.git
RUN chown -R build yay
WORKDIR /yay
USER build
RUN makepkg -si --noconfirm
RUN yay -S --noconfirm mingw-w64-crt-bin \
  mingw-w64-binutils-bin \
  mingw-w64-winpthreads-bin \
  mingw-w64-headers-bin
RUN yay -S --noconfirm mingw-w64-gcc-bin

USER root
RUN rustup toolchain install nightly
RUN rustup component add rust-src
RUN rustup component add rustfmt
RUN cargo install cargo-xbuild

WORKDIR /src