# Use the official Rust image as the base
FROM rust:latest AS builder

# Set working directory
WORKDIR /app

# Copy the Cargo.toml and Cargo.lock files first (cache dependencies)
COPY Cargo.toml Cargo.lock ./

# Create a dummy source file to force Cargo to only download dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Fetch dependencies
RUN cargo build --release

# Remove the temporary source file
RUN rm -rf src

# Copy the actual source code
COPY ./src ./src

# Build the Rust binary
RUN cargo build --release

# Use a minimal base image for the runtime
FROM debian:bullseye-slim

# Set working directory
WORKDIR /app

# Install necessary runtime dependencies (adjust based on your binary needs)
RUN apt-get update && apt-get install -y \
    libssl-dev \
    ca-certificates \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Copy the built binary from the builder stage
COPY --from=builder /app/target/release/super-fast-smb-image-uploader .

# Copy other necessary files
COPY .env docker-compose.yaml LICENSE README.md /app/

# Specify the entry point
ENTRYPOINT ["./super-fast-smb-image-uploader"]

# Expose necessary ports (adjust as needed)
EXPOSE 8080
