# Build rust application
FROM rust:latest 

# Copy the source code
COPY ./ ./

# Build the application
RUN cargo build --release

# Define the command to run
CMD ["./target/release/main"]