# lorust

> <u>**lo**</u>ad generator <u>**rust**</u>

A load generator tool written in Rust. Currently supports
http api calls and custom scripting support with RustPython.

## Build

```sh
$ cargo build --release
$ target/release/lorust --help
```

## Usage

For simple HTTP load tests, use the `http` command:

```sh
$ target/release/lorust http https://example.com -n 100 -r 10 --output-path metrics.json
```

This starts 100 total requests at 10 requests per second and writes
request metrics to `metrics.json`.

Metrics include `run_id`, `worker_id`, `task_id`, and `sequence` fields so
outputs from multiple workers can be merged later. If `--run-id` is omitted,
lorust generates one for the local run. If `--worker-id` is omitted, it uses
`local`.

Common options:

```sh
$ target/release/lorust http https://api.example.com/users \
    -n 50 \
    -r 5 \
    --run-id local-check \
    --worker-id laptop \
    -m POST \
    -H 'Content-Type: application/json' \
    -d '{"name":"Ada"}' \
    --timeout 10 \
    --output-path metrics.json
```

For scripted flows, use a JSON flow file:

```sh
$ target/release/lorust run --flow-path flow.json --output-path metrics.json
```

The legacy top-level `--flow` and `--flow-path` flags are still supported.

Example `HttpRequest`

```json
{
    "HttpRequest": {
        "method": "POST",
        "url": "https://reqres.in/api/users?page=1",
        "headers": [
            ["Content-Type", "application/json"],
            ["X-ACCESS-TOKEN", "32808ft6-21e4-4gh0-8dad-2348987838"]
        ],
        "body": "...",
        "redirect_limit": 5,
        "timeout": 300
    }
}
```

Example config (this will likely change):

```json
{
    "functions": [
        {
            "LoadGen": {
                "max_tasks": 2,
                "spawn_rate": "1",
                "timeout": 300,
                "functions_to_execute": [
                    {
                        "HttpRequest": {
                            "url": "https://reqres.in/api/users?page=1",
                            "timeout": 300
                        }
                    },
                    {
                        "RunPythonCode": {
                            "code": "user_id = http_response[\"data\"][0][\"id\"]"
                        }
                    },
                    {
                        "RunPythonCode": {
                            "code": "print(f\"Picked user_id: {user_id}\")"
                        }
                    },
                    {
                        "HttpRequest": {
                            "url": "https://reqres.in/api/users/%|user_id|%",
                            "timeout": 300
                        }
                    },
                    {
                        "RunPythonCode": {
                            "code": "data = http_response[\"data\"]; print(data[\"first_name\"] + \" \" + data[\"last_name\"])"
                        }
                    }
                ]
            }
        }
    ]
}
```

The above config gives the following output:

```
--- Running function #1 ---
Running load generator with the config:
LoadGenParam { spawn_rate: "1", timeout: 300, max_tasks: Some(2), functions_to_execute: [] }
Picked user_id: 3
Emma Wong
Picked user_id: 4
Eve Holt
=== Load test complete ===
TOTAL TASKS: 2
PASSED: 2
FAILED: 0
Collected metrics array size: 4
=== HTTP metric summary ===
REQUESTS: 4
HTTP 2XX: 4
HTTP NON-2XX/ERROR: 0
ERROR RATE: 0.00%
REQUESTS/SEC: 1.98
AVG LATENCY MS: 48.50
P50 LATENCY MS: 47
P95 LATENCY MS: 56
P99 LATENCY MS: 56
Saving collected metrics to: "metrics.json"
```
