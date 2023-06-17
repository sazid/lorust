# lorust

> <u>**lo**</u>ad generator <u>**rust**</u>

A load generator tool written in Rust. Currently supports
http api calls and custom scripting support with Rhai.

**Note**: The API to the outside world has not been determined yet.
Currently, only json based config is supported but this may also
change to support other config formats such as TOML, etc. depending
on whether they'll be able to meet the needs and make the config
more human readable.

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
                "spawn_rate": "1",
                "timeout": 1,
                "functions_to_execute": [
                    {
                        "HttpRequest": {
                            "url": "https://reqres.in/api/users?page=1",
                            "timeout": 300
                        }
                    },
                    {
                        "RunRhaiCode": {
                            "code": "let user_id = http_response[\"data\"].sample().id;"
                        }
                    },
                    {
                        "RunRhaiCode": {
                            "code": "print(\"Picked user_id: \" + user_id);"
                        }
                    },
                    {
                        "HttpRequest": {
                            "url": "https://reqres.in/api/users/%|user_id|%",
                            "timeout": 300
                        }
                    },
                    {
                        "RunRhaiCode": {
                            "code": "let data = http_response.data; print(data.first_name + \" \" + data.last_name);"
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
LoadGenParam { spawn_rate: "1", timeout: 1, max_tasks: None, functions_to_execute: [] }
=== TICK #1, TASK COUNT: 1 ===
=== TICK #2, TASK COUNT: 1 ===
Picked user_id: 3
Emma Wong
Picked user_id: 4
Eve Holt
Load test complete.
TOTAL TASKS: 2
PASSED: 2
FAILED: 0
Collected metrics array size: 4
Printing first 3 entries
```

```json
[
    {
        "url": "https://reqres.in/api/users?page=1",
        "http_verb": "GET",
        "status_code": 200,
        "response_body_size": 996,
        "time_stamp": "2023-06-17 18:41:47.572695000",
        "response_body": "",
        "upload_total": 0,
        "download_total": 368,
        "upload_speed": 0.0,
        "download_speed": 339.0,
        "namelookup_time": {
            "secs": 1,
            "nanos": 27096000
        },
        "connect_time": {
            "secs": 0,
            "nanos": 6369000
        },
        "tls_handshake_time": {
            "secs": 0,
            "nanos": 35801000
        },
        "starttransfer_time": {
            "secs": 1,
            "nanos": 83679000
        },
        "elapsed_time": {
            "secs": 1,
            "nanos": 83827000
        },
        "redirect_time": {
            "secs": 0,
            "nanos": 0
        }
    },
    {
        "url": "https://reqres.in/api/users/3",
        "http_verb": "GET",
        "status_code": 200,
        "response_body_size": 274,
        "time_stamp": "2023-06-17 18:41:48.660536000",
        "response_body": "",
        "upload_total": 0,
        "download_total": 208,
        "upload_speed": 0.0,
        "download_speed": 7407.0,
        "namelookup_time": {
            "secs": 0,
            "nanos": 1638000
        },
        "connect_time": {
            "secs": 0,
            "nanos": 5441000
        },
        "tls_handshake_time": {
            "secs": 0,
            "nanos": 8274000
        },
        "starttransfer_time": {
            "secs": 0,
            "nanos": 27956000
        },
        "elapsed_time": {
            "secs": 0,
            "nanos": 28080000
        },
        "redirect_time": {
            "secs": 0,
            "nanos": 0
        }
    },
    {
        "url": "https://reqres.in/api/users?page=1",
        "http_verb": "GET",
        "status_code": 200,
        "response_body_size": 996,
        "time_stamp": "2023-06-17 18:41:48.572680000",
        "response_body": "",
        "upload_total": 0,
        "download_total": 368,
        "upload_speed": 0.0,
        "download_speed": 344.0,
        "namelookup_time": {
            "secs": 0,
            "nanos": 27449000
        },
        "connect_time": {
            "secs": 1,
            "nanos": 15540000
        },
        "tls_handshake_time": {
            "secs": 0,
            "nanos": 9175000
        },
        "starttransfer_time": {
            "secs": 1,
            "nanos": 67105000
        },
        "elapsed_time": {
            "secs": 1,
            "nanos": 67367000
        },
        "redirect_time": {
            "secs": 0,
            "nanos": 0
        }
    }
]
```
