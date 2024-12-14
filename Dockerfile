# 1. Use the official Rust image as the builder stage
FROM rust:1.83 as builder

# 2. Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    zlib1g-dev \
    cmake \
    git && \
    rm -rf /var/lib/apt/lists/*

# 3. Set the working directory inside the container
WORKDIR /usr/src/app

# 4. Copy only necessary files for dependency resolution
COPY Cargo.toml Cargo.lock ./

# 5. Fetch dependencies with verbose output
RUN cargo fetch --verbose

# 6. Copy the source code and build the binary
COPY src ./src
RUN cargo build --release --locked --verbose --target-dir=/usr/src/app/target

# 7. Use a compatible runtime image for the final stage
FROM debian:bookworm-slim

# 8. Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl-dev \
    ca-certificates \
    zlib1g && \
    rm -rf /var/lib/apt/lists/*

# 9. Set the working directory in the runtime container
WORKDIR /app

# 10. Copy the built binary from the builder stage
COPY --from=builder /usr/src/app/target/release/super-fast-smb-image-uploader ./

# 11. Run the application as a non-root user for better security
RUN useradd --no-create-home rustuser
USER rustuser

# 12. Define the command to run the application
CMD ["./super-fast-smb-image-uploader"]
