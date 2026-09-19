# nlgrep

Natural-language grep for classifying input lines with Jev through TypeSafe.

## Usage

```shell
TYPESAFE_API_KEY=ts_example_key nlgrep "select fruit" words.txt
cat words.txt | nlgrep --json --all "select fruit"
nlgrep --threshold 0.8 --concurrency 1 "select fruit" words.txt
```

`QUERY` is a natural-language request. With no `FILE` arguments, `nlgrep`
reads stdin. A file argument of `-` also means stdin. Input is sent to the
configured TypeSafe API as UTF-8 text, so do not put secrets in input files.

The API key is read from `TYPESAFE_API_KEY`. The compiled-in adapter uses
`https://api.typesafe.ai/v1/systemone` and the `jev-latest` model by default;
the request follows the [TypeSafe HTTP API](https://docs.typesafe.ai/api).

## Options

| Option | Default | Description |
| --- | --- | --- |
| `-t, --threshold FLOAT` | `0.5` | Match when Jev's `noul` probability is at least this value. The value must be in `[0, 1]`. |
| `-j, --json` | off | Emit JSON Lines instead of plain text. |
| `--all` | off | With `--json`, emit a decision for every input line, including non-matches. |
| `--model MODEL` | `jev-latest` | TypeSafe model identifier. |
| `--timeout DURATION` | `30s` | Timeout for one provider request, for example `10s` or `500ms`. |
| `--concurrency INT` | `4` | Maximum in-flight batches. `1` is sequential; `0` removes only the local cap and schedules all planned batches immediately. The value must be non-negative. |

The provider's batch limit, request timeout, retry policy, and server-side
limits still apply when `--concurrency 0` is selected.

## Output

Plain mode prints only matching lines. When multiple files are selected, each
line is prefixed with its source path:

```text
first.txt:banana
second.txt:apple
```

JSON mode emits one object per line:

```json
{"file":null,"line":2,"text":"banana","matched":true,"probability":0.98}
```

`file` is `null` for stdin, `line` is one-based, and `text` excludes the input
line terminator. `--all` is invalid without `--json`.

Input is strict UTF-8. Empty lines and duplicate lines are valid records, and
output order follows input order even when batches are processed concurrently.

## Exit codes

- `0`: at least one line matched and no input or provider error occurred.
- `1`: no line matched and no error occurred.
- `2`: invalid arguments, input error, missing API key, provider error, or
  output error.

The first version intentionally has no regular-expression mode, ranking, local
model fallback, query file, or disk cache.
