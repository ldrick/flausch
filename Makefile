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

apk:            ## Build the debug app "Flausch Dev" (target/debug/apk/flausch-dev.apk)
	cargo apk build -p flausch-dev --lib --target $(TARGET)

# Without absolute paths in the binary, the APK is the same wherever it is built (reproducible for F-Droid).
# CI and F-Droid use a minimal toolchain; with rust-src installed, rustc would write its local path instead of
# /rustc/<commit>. TZ: cargo-apk stores the ZIP timestamps in local time.
RUST_SRC    = $(shell rustc --print sysroot)/lib/rustlib/src/rust
RUST_COMMIT = $(shell rustc -vV | sed -n 's/^commit-hash: //p')
apk-release:    ## Build the release APK of Flausch (target/release/apk/flausch.apk)
	TZ=UTC RUSTFLAGS="--remap-path-prefix=$(CURDIR)=/build --remap-path-prefix=$${CARGO_HOME:-$$HOME/.cargo}=/cargo \
	  --remap-path-prefix=$(RUST_SRC)=/rustc/$(RUST_COMMIT)" \
	  cargo apk build --lib --release --target $(TARGET)

install:        ## Build, install and start "Flausch Dev" on a connected device/emulator
	cargo apk run -p flausch-dev --lib --target $(TARGET)

emulator:       ## Start the Android emulator (then: make install TARGET=x86_64-linux-android)
	$(ANDROID_HOME)/emulator/emulator -avd $(AVD)

clean:
	cargo clean
