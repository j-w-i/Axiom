# Fixtures

## `lmstudio-recorded/`: recorded
From LM Studio 0.4.25+1 (the bundle's version) at `127.0.0.1:1234` on macOS 27 arm64, on 2026-11-24 23:41–23:44 UTC (2026-11-25 local),
by `examples/record.rs`. MLX models, loaded by LM Studio when asked for. No key was set. `*.http` is the answer byte for byte: head,
chunk framing, body. `*.request.json` is the body that was sent.

| Name | Model | What happened |
|------|-------|---------------|
| `text` | `qwen2.5-0.5b-instruct-mlx` | One word, `finish_reason` `stop`, usage in an event of its own after it |
| `text-second-model` | `qwen3.5-0.8b-mlx` | A model that can reason; it did not, and nothing here has reasoning in it |
| `length` | `qwen2.5-0.5b-instruct-mlx` | Cut by `max_tokens` (6; it says 5 were used) |
| `stop-sequence` | `qwen2.5-0.5b-instruct-mlx` | Ended by a stop sequence, which is reported as `stop` like any other end |
| `tool` | `qwen/qwen3-4b-2507` | One call, arguments in pieces |
| `tool-parallel` | `qwen/qwen3-4b-2507` | Two calls, with the text `"\n"` between them |
| `tool-no-args` | `qwen/qwen3-4b-2507` | A call with `{}` |
| `tool-result` | `qwen/qwen3-4b-2507` | Text after a tool's result |
| `tool-cut` | `qwen/qwen3-4b-2507` | A call cut by `max_tokens` (12): arguments `""`, `finish_reason` `length` |
| `unknown-model` | asked for `no-such-model` | **Answered with 200 by `qwen/qwen3-4b-2507`**, which was loaded; only the stream's `model` says so |
| `too-long` | `qwen2.5-0.5b-instruct-mlx` | **200, then `event: error`** with no code; the prompt was `"word "` 40 000 times, and is not kept here |
| `error-request` | `qwen2.5-0.5b-instruct-mlx` | 400, `{"error": "…"}`: no messages |
| `models` | – | `GET /v1/models` |

## `messages-from-documentation/`: from documentation, not recorded
There is no Anthropic key here. These are the example streams of
<https://platform.claude.com/docs/en/build-with-claude/streaming>, read on 2026-11-25, as printed there, except that a blank line ends the
last event where the page leaves it out, and that `web-search` is made whole where the page has `...`: the list of results closes after its first, and
the blocks between index 3 and 17 are left out, the last one's `content_block_stop` having index 3. Identifiers and signatures are the page's own,
shortened by it. Bodies of errors in the tests are from `api/errors` and `api/rate-limits`, the redacted block is in the form of
`build-with-claude/thinking`. The API's real behaviour has not been tested.

## `openrouter-from-documentation/`: from documentation, not recorded
There is no OpenRouter key here. Read on 2026-11-25:

| Name | Page | What was done to it |
|------|------|---------------------|
| `error` | <https://openrouter.ai/docs/api_reference/errors-and-debugging#mid-stream-errors> | As printed |
| `reasoning` | <https://openrouter.ai/docs/guides/best-practices/reasoning-tokens#streaming-response> | **Put together**: that piece, then the last events of <https://openrouter.ai/docs/api_reference/streaming#the-final-usage-chunk-chat-completions>, whose `...` are left out and whose `usage` is the example of <https://openrouter.ai/docs/cookbook/administration/usage-accounting>; before them the comment that page says is sent |

The APIs' real behaviour has not been tested.
