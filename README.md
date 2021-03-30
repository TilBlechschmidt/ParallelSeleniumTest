# Simple Selenium test dispatcher

This is a tool that simply runs a number of trivial Selenium tests against a given endpoint (in parallel). You provide the endpoint and number of forks via the commandline as first and second argument. Additionally, you can pass a browser name (either `firefox` or `chrome`) as the third argument.

Example invokations:

```bash
# 5x Firefox against local grid
cargo run -- http://localhost:8080/ 5
cargo run -- http://localhost:8080/ 5 firefox

# 5x Chrome
cargo run -- http://localhost:8080/ 5 chrome

# Same story but with Docker
docker run --rm -it -e ENDPOINT=http://example.com -e FORKS=5 -e BROWSER=chrome ghcr.io/tilblechschmidt/parallelseleniumtest:sha-b0e4408c
```
