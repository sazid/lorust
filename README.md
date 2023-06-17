# ZeuZ Node Native

A load generator tool written in Rust. Currently supports
http api calls and custom scripting support with Rhai.

**Note**: The API to the outside world has not been determined yet.
Currently, only json based config is supported but this may also
change to support other config formats such as TOML, etc. depending
on whether they'll be able to meet the needs and make the config
more human readable.

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
