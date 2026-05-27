# lorust

> **lo**ad generator **rust**

`lorust` is a Rust load generator for HTTP APIs. It supports simple CLI-driven
HTTP load tests and TOML flow files with RustPython scripting between actions.

## Build

```sh
cargo build --release
target/release/lorust --help
```

For development, replace `target/release/lorust` with:

```sh
cargo run --
```

## Quick Start

Run 100 total requests at 10 requests per second:

```sh
target/release/lorust http https://example.com \
    -n 100 \
    -r 10 \
    --output-path metrics.json
```

Run for 60 seconds at 10 requests per second:

```sh
target/release/lorust http https://example.com \
    --duration 60s \
    -r 10 \
    --output-path metrics.json
```

Send a POST request with headers and a raw body:

```sh
target/release/lorust http https://api.example.com/users \
    -n 50 \
    -r 5 \
    -m POST \
    -H 'Content-Type: application/json' \
    -d '{"name":"Ada"}' \
    --timeout 10 \
    --output-path metrics.json
```

Fail the process when thresholds are missed:

```sh
target/release/lorust http https://example.com \
    --duration 30s \
    -r 20 \
    --max-error-rate 1 \
    --max-p95-ms 500 \
    --min-rps 18 \
    --output-path metrics.json
```

## CLI Modes

`lorust` has two primary modes:

- `http`: run a simple HTTP load test without writing a flow file.
- `run`: run a TOML or JSON flow definition.

Legacy top-level `--flow` and `--flow-path` flags are still supported.

## Global Options

These flags work with both `http` and `run`.

| Option | Description | Default |
| --- | --- | --- |
| `--output-path <path>` | Write raw HTTP metrics JSON to this path. | `metrics_output` |
| `--run-id <id>` | Logical run ID attached to every emitted metric. | generated |
| `--worker-id <id>` | Worker ID attached to every emitted metric. | `local` |

Example:

```sh
target/release/lorust \
    --run-id local-check \
    --worker-id laptop \
    http https://example.com -n 10 -r 2
```

## HTTP Command

Usage:

```sh
target/release/lorust http [OPTIONS] <URL>
```

| Option | Description | Default |
| --- | --- | --- |
| `<URL>` | URL to request. | required |
| `-n, --requests <n>` | Total number of requests to execute. | `1` when `--duration` is absent |
| `-r, --rate <n>` | Requests to start per second. | `1` |
| `--duration <duration>` | Start requests for a duration. Supports plain seconds, `s`, `m`, and `h`. | unset |
| `-m, --method <method>` | HTTP method. Defaults to `POST` when `--body` is provided, otherwise `GET`. | `GET` or `POST` |
| `-H, --header <header>` | Header in `Name: value` format. Can be passed multiple times. | none |
| `-d, --body <body>` | Raw request body. | none |
| `--timeout <seconds>` | Per-request timeout in seconds. | `30` |
| `--redirect-limit <n>` | Maximum redirects to follow. | `5` |
| `--max-response-body-bytes <n>` | Maximum failed-response body bytes to store in metrics. | `4096` |
| `--max-error-rate <percent>` | Fail if error rate is above this percentage. | unset |
| `--max-p95-ms <ms>` | Fail if p95 latency is above this many milliseconds. | unset |
| `--max-p99-ms <ms>` | Fail if p99 latency is above this many milliseconds. | unset |
| `--min-rps <rps>` | Fail if achieved requests/sec is below this value. | unset |

`--requests` and `--duration` are mutually exclusive. Use request-count mode for
fixed-size tests and duration mode for time-window tests.

## Output

`lorust` prints a human-readable summary to stdout:

```text
=== Load test complete ===
TOTAL TASKS: 100
PASSED: 100
FAILED: 0
Collected metrics array size: 100
=== HTTP metric summary ===
REQUESTS: 100
HTTP 2XX: 100
HTTP NON-2XX/ERROR: 0
ERROR RATE: 0.00%
REQUESTS/SEC: 9.98
AVG LATENCY MS: 42.10
P50 LATENCY MS: 40
P95 LATENCY MS: 75
P99 LATENCY MS: 91
Saving collected metrics to: "metrics.json"
```

It also writes raw per-request metrics to `--output-path` as a JSON array.

Each HTTP metric includes:

- `run_id`
- `worker_id`
- `task_id`
- `sequence`
- `url`
- `http_verb`
- `status_code`
- `response_body_size`
- `time_stamp`
- `started_at_nanos`
- `response_body`
- `response_body_truncated`
- upload/download totals and speeds
- DNS/connect/TLS/start-transfer/elapsed/redirect timings in milliseconds

Metrics are sorted by:

1. `started_at_nanos`
2. `worker_id`
3. `task_id`
4. `sequence`

This makes local output easier to inspect and prepares the format for future
multi-worker aggregation.

## Exit Behavior

The process exits successfully when the load test passes and all configured
thresholds pass.

The process exits non-zero when:

- any task fails
- HTTP failures make tasks fail
- a configured threshold fails
- a flow or CLI argument is invalid

Even when a threshold fails, `lorust` still writes the metrics file before
returning the error.

## Flow Files

Use flow files for multi-step scenarios, scripting, or variable interpolation.
TOML is the preferred authoring format because multiline Python code can be
written as literal strings. JSON flow files are still supported for compatibility.

Run a flow file:

```sh
target/release/lorust run --flow-path flow.toml --output-path metrics.json
```

Run an inline JSON flow:

```sh
target/release/lorust run \
    --flow '{"functions":[{"LoadGen":{"max_tasks":1,"spawn_rate":"1","timeout":5,"functions_to_execute":[{"Sleep":{"duration":"1"}}]}}]}'
```

Legacy form:

```sh
target/release/lorust --flow-path flow.toml --output-path metrics.json
```

Flow files are parsed by extension. Use `.toml` for the readable flow syntax and
`.json` for the legacy enum-shaped JSON syntax.

### Load Generator

Each `[[loadgen]]` table schedules virtual-user tasks.

Common fields:

| Field | Description |
| --- | --- |
| `spawn_rate` | Python expression returning tasks to start per second. `TICK` is available. |
| `timeout` | Per virtual-user timeout in seconds. |
| `max_tasks` | Total tasks to start. |
| `duration` | Seconds to start tasks for. |
| `run_id` | Optional run ID. CLI `--run-id` overrides this. |
| `worker_id` | Optional worker ID. CLI `--worker-id` overrides this. |
| `thresholds` | Optional summary threshold object. |
| `step` | Ordered `[[loadgen.step]]` tables each virtual user executes. |

`max_tasks` and `duration` can both be omitted only if the CLI path supplies a
default. In flow files, provide at least one.

Request-count example:

```toml
[[loadgen]]
max_tasks = 100
spawn_rate = "10"
timeout = 30

[[loadgen.step]]
type = "http"
url = "https://example.com"
timeout = 10
```

Duration example:

```toml
[[loadgen]]
duration = 60
spawn_rate = "10"
timeout = 30

[[loadgen.step]]
type = "http"
url = "https://example.com"
timeout = 10
```

Threshold example:

```toml
[[loadgen]]
duration = 30
spawn_rate = "20"
timeout = 30

[loadgen.thresholds]
max_error_rate = 1.0
max_p95_latency_ms = 500
max_p99_latency_ms = 1000
min_requests_per_sec = 18.0

[[loadgen.step]]
type = "http"
url = "https://example.com"
```

### HTTP Step

`type = "http"` performs one HTTP request.

Fields:

| Field | Description | Default |
| --- | --- | --- |
| `url` | Request URL. | required |
| `method` | HTTP method. | `GET` |
| `headers` | Inline table of HTTP header names to values. | `{}` |
| `body` | Raw request body string. | empty |
| `form_data` | Inline table of multipart form field names to string values or file tables. | unset |
| `form_urlencoded` | Inline table of form field names to values. | unset |
| `timeout` | Request timeout in seconds. | `60` |
| `redirect_limit` | Maximum redirects to follow. | `5` |
| `max_response_body_bytes` | Maximum failed-response body bytes to store in metrics. | `4096` |

GET example:

```toml
[[loadgen.step]]
type = "http"
url = "https://example.com"
timeout = 10
```

POST with raw JSON body:

```toml
[[loadgen.step]]
type = "http"
method = "POST"
url = "https://api.example.com/users"
headers = { "Content-Type" = "application/json" }
body = '''
{"name":"Ada"}
'''
timeout = 10
max_response_body_bytes = 4096
```

Only one body style can be used on an HTTP step:

- omit `body`, `form_data`, and `form_urlencoded` for an empty body
- `body = '''...'''` for a raw request body
- `form_data = { name = "Ada" }` for multipart form fields
- `form_data = { avatar = { file_path = "avatar.png", content_type = "image/png" } }` for multipart files
- `form_urlencoded = { name = "Ada" }` for URL-encoded form bodies

`BinaryOctetFilePath` exists in the type but is not implemented yet.

### Python Step

`type = "python"` runs RustPython code against the current virtual-user local
scope.

Values assigned in Python are written back to the virtual-user scope when they
can be serialized to JSON.

Example:

```toml
[[loadgen.step]]
type = "python"
code = '''
user_id = http_response["data"][0]["id"]
'''
```

### Variable Interpolation

Strings can interpolate Python expressions with `%|...|%`.

Example:

```toml
[[loadgen.step]]
type = "http"
url = "https://api.example.com/users/%|user_id|%"
```

The expression is evaluated against the virtual-user local scope.

### Sleep

`type = "sleep"` pauses the virtual user for a number of seconds.

```toml
[[loadgen.step]]
type = "sleep"
duration = "1"
```

## Multi-Step Flow Example

```toml
[[loadgen]]
max_tasks = 2
spawn_rate = "1"
timeout = 300

[[loadgen.step]]
type = "http"
url = "https://reqres.in/api/users?page=1"
timeout = 300

[[loadgen.step]]
type = "python"
code = '''
user_id = http_response["data"][0]["id"]
print(f"Picked user_id: {user_id}")
'''

[[loadgen.step]]
type = "http"
url = "https://reqres.in/api/users/%|user_id|%"
timeout = 300

[[loadgen.step]]
type = "python"
code = '''
data = http_response["data"]
print(data["first_name"] + " " + data["last_name"])
'''
```

## Distributed Work

`lorust` is still a single-node load generator today. The metric format already
includes `run_id`, `worker_id`, `task_id`, and `sequence` so future
supervisor/worker aggregation can merge outputs safely.

The distributed implementation plan lives in:

```text
docs/distributed-load-testing-plan.md
```
