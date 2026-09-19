# nlgrep

Natural-language grep for classifying input lines with Jev through TypeSafe.

## Installation

### Prebuilt binaries

The recommended installation method is to download a binary from the
[GitHub Releases](https://github.com/AlexFives/nlgrep/releases) page. Each
release includes these targets:

| Platform | Architecture | Target | Archive |
| --- | --- | --- | --- |
| Linux | x86_64 | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| Linux | ARM64 | `aarch64-unknown-linux-gnu` | `.tar.gz` |
| macOS | Intel | `x86_64-apple-darwin` | `.tar.gz` |
| macOS | Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` |
| Windows | x86_64 | `x86_64-pc-windows-msvc` | `.zip` |

Archive names follow the pattern
`nlgrep-<release-tag>-<target>.<archive-extension>`.

On Linux or macOS, select the target for your machine and run:

```shell
VERSION=v0.1.0
TARGET=x86_64-unknown-linux-gnu
ARCHIVE="nlgrep-${VERSION}-${TARGET}.tar.gz"
RELEASES_URL="https://github.com/AlexFives/nlgrep/releases/download"

curl --fail --location \
  "${RELEASES_URL}/${VERSION}/${ARCHIVE}" \
  --output "$ARCHIVE"
tar --extract --gzip --file "$ARCHIVE"
sudo install -m 0755 "nlgrep-${VERSION}-${TARGET}/nlgrep" /usr/local/bin/nlgrep
```

Replace `VERSION` and `TARGET` with the release and platform you want. For
Apple Silicon use `aarch64-apple-darwin`; for Intel macOS use
`x86_64-apple-darwin`; for Linux ARM64 use `aarch64-unknown-linux-gnu`.

On Windows, download the matching `.zip` archive, extract it, and put the
directory containing `nlgrep.exe` on `PATH`:

```powershell
$Version = "v0.1.0"
$Target = "x86_64-pc-windows-msvc"
$Archive = "nlgrep-$Version-$Target.zip"
$Url = "https://github.com/AlexFives/nlgrep/releases/download/$Version/$Archive"

Invoke-WebRequest -Uri $Url -OutFile $Archive
Expand-Archive -Path $Archive -DestinationPath .
```

### Build from source

If a prebuilt target is not available, install Rust 1.95 or newer and run:

```shell
cargo install --locked --git https://github.com/AlexFives/nlgrep nlgrep
```

### Configure the API key

Set `TYPESAFE_API_KEY` before using `nlgrep`:

```shell
export TYPESAFE_API_KEY=ts_example_key
```

In PowerShell:

```powershell
$env:TYPESAFE_API_KEY = "ts_example_key"
```

Verify the installation with:

```shell
nlgrep --version
nlgrep --help
```

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
| `-t, --threshold FLOAT` | `0.5` | Match when `noul` is at least the value. |
| `-j, --json` | off | Emit JSON Lines instead of plain text. |
| `--all` | off | Emit all lines in JSON mode, including non-matches. |
| `--model MODEL` | `jev-latest` | TypeSafe model identifier. |
| `--timeout DURATION` | `30s` | Request timeout, e.g. `10s` or `500ms`. |
| `--concurrency INT` | `4` | In-flight limit; `0` removes the local cap. |

`--threshold` must be in `[0, 1]`.
`--concurrency 1` processes batches sequentially. The value must be
non-negative. The provider's batch limit, request timeout, retry policy, and
server-side limits still apply when `--concurrency 0` is selected.

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
