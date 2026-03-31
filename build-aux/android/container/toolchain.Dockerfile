FROM debian:sid

ENV DEBIAN_FRONTEND noninteractive
RUN apt-get -y update && apt-get -y upgrade && apt-get -y install --no-install-recommends \
    build-essential \
    gir1.2-appstream \
    git \
    glslc \
    gobject-introspection \
    libglib2.0-dev-bin \
    libglib-perl \
    libglib-object-introspection-perl \
    libipc-run-perl \
    libjson-perl \
    libset-scalar-perl \
    libxml2-utils \
    libxml-libxml-perl \
    libxml-libxslt-perl \
    meson \
    ninja-build \
    openjdk-21-jre \
    openjdk-21-jdk \
    perl \
    sassc \
    sdkmanager \
 && apt-get -y clean

ENV ANDROID_HOME /opt/android/
ENV ANDROID_SDKVER 35.0.0
ENV ANDROID_NDKVER 29.0.14206865

ENV GOOGLE_RUST_BRANCH android-16.0.0_r4
ENV RUST_VERSION 1.88.0

COPY android-sdk.sh android-rust.sh .
RUN ./android-sdk.sh
RUN ./android-rust.sh

RUN git clone https://github.com/sp1ritCS/mini-studio.git --depth 1 /opt/mini-studio

ENV LANG C.UTF-8
