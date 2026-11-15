
# Build everything, but don't launch.
compile:
    cargo build --workspace

# Build everything, then start the app.
dev:
    cargo build --workspace
    cargo run -p launcher

# Build the optimized application.
build:
    cargo build --workspace --release
