# 1. Use the official Rust image as the builder stage
FROM rust:1.83 as builder

# 2. Set the working directory inside the container
WORKDIR /usr/src/app

# 3. Copy only the necessary files to leverage Docker's caching mechanism
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# 4. Build the program in release mode
RUN cargo build --release

# 5. Use a compatible runtime image
FROM debian:bookworm-slim

# 6. Set the working directory in the runtime container
WORKDIR /app

# 7. Install necessary libraries
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

# 8. Copy the built binary from the builder stage
COPY --from=builder /usr/src/app/target/release/super-fast-smb-image-uploader ./

# 9. Specify a non-root user for better security (optional)
RUN useradd -m rustuser
USER rustuser

# 10. Define the command to run the application
CMD ["./super-fast-smb-image-uploader"]
