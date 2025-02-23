To build the Docker container, be careful to set `host.docker.internal` as a IP, instead of `localhost`, and execute:
```bash
docker build -t elevator .
```

To run the Docker container:

```bash
docker run -d elevator
```