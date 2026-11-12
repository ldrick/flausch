# Build and run Flausch. Paths can be overridden: make apk ANDROID_HOME=… JAVA_HOME=…
# The SDK versions CI and F-Droid use, so build.rs doesn't pick whatever is newest.
include android/sdk.env
ANDROID_HOME     ?= $(HOME)/Android/Sdk
# The pinned NDK if it is installed, otherwise the newest one.
ANDROID_NDK_ROOT ?= $(or $(wildcard $(ANDROID_HOME)/ndk/$(ANDROID_NDK_VERSION)),$(lastword $(sort $(wildcard $(ANDROID_HOME)/ndk/*))))
JAVA_HOME        ?= $(HOME)/.local/opt/jdk-21
TARGET           ?= aarch64-linux-android
AVD              ?= flausch

export ANDROID_HOME ANDROID_NDK_ROOT JAVA_HOME ANDROID_PLATFORM ANDROID_BUILD_TOOLS_VERSION
export PATH := $(HOME)/.cargo/bin:$(PATH)

.PHONY: run test check check-android deny ci apk apk-release install emulator clean

run:            ## Run the app on the desktop
	cargo run

test:           ## Run unit tests
	cargo test

check:          ## Format check and lints
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings

check-android:  ## Lints for the Android code (platform/android.rs, android_main)
	cargo clippy --lib --target $(TARGET) -- -D warnings

deny:           ## Advisories, licenses and sources of dependencies (needs cargo-deny)
	cargo deny check

ci: check check-android test deny  ## Everything CI runs, except the APK build

apk:            ## Build a debug APK (target/debug/apk/flausch.apk)
	cargo apk build --lib --target $(TARGET)

apk-release:    ## Build a release APK
	cargo apk build --lib --release --target $(TARGET)

install:        ## Build, install and start on a connected device/emulator
	cargo apk run --lib --target $(TARGET)

emulator:       ## Start the Android emulator (then: make install TARGET=x86_64-linux-android)
	$(ANDROID_HOME)/emulator/emulator -avd $(AVD)

clean:
	cargo clean
