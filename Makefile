# Build and run nøste. Paths can be overridden: make apk ANDROID_HOME=… JAVA_HOME=…
ANDROID_HOME     ?= $(HOME)/Android/Sdk
ANDROID_NDK_ROOT ?= $(lastword $(sort $(wildcard $(ANDROID_HOME)/ndk/*)))
JAVA_HOME        ?= $(HOME)/.local/opt/jdk-21
TARGET           ?= aarch64-linux-android
AVD              ?= noste

export ANDROID_HOME ANDROID_NDK_ROOT JAVA_HOME
export PATH := $(HOME)/.cargo/bin:$(PATH)

.PHONY: run test check apk apk-release install emulator clean

run:            ## Run the app on the desktop
	cargo run

test:           ## Run unit tests
	cargo test

check:          ## Format check and lints
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings

apk:            ## Build a debug APK (target/debug/apk/noste.apk)
	cargo apk build --lib --target $(TARGET)

apk-release:    ## Build a release APK
	cargo apk build --lib --release --target $(TARGET)

install:        ## Build, install and start on a connected device/emulator
	cargo apk run --lib --target $(TARGET)

emulator:       ## Start the Android emulator (then: make install TARGET=x86_64-linux-android)
	$(ANDROID_HOME)/emulator/emulator -avd $(AVD)

clean:
	cargo clean
