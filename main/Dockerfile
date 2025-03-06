# Build rust application
FROM rust:latest 

# Set the working directory
WORKDIR /usr/src/app

# Copy the source code
COPY ./ ./
COPY entrypoint.sh /usr/src/app/entrypoint.sh

# Build the application
RUN cargo build --release

# Define the entrypoint to run the application
ENTRYPOINT ["./entrypoint.sh"]