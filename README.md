# OpenWhisper Cleanup Server

Text cleanup server for [OpenWhispr](https://github.com/OpenWhispr/openwhispr). Takes raw speech-to-text output and fixes it up - adds punctuation, fixes spelling, removes filler words, and optionally translates Polish to English.

Exposes an OpenAI-compatible chat completions API so OpenWhispr can talk to it as a custom provider.

## What it does

The server runs text through an 8-stage pipeline:

1. Language detection (English or Polish)
2. Filler word removal ("um", "uh", "no wiesz", etc.)
3. Number normalization ("twenty three" -> "23")
4. Punctuation and capitalization restoration (ONNX model)
5. Spelling corrections (common mistakes per language)
6. User-defined word replacements (hot-reloaded from config)
7. Grammar checking (via LanguageTool)
8. Translation (Polish -> English, optional)

Each stage runs in order. If one fails, the rest keep going.

## Quick start with Docker Compose

```bash
mise run setup-models   # download ONNX model + generate tokenizers
docker compose up -d --build
```

This starts the cleanup server and LanguageTool together. The API is at `http://localhost:8787`. LanguageTool runs internally and isn't exposed.

## Running locally

Needs Rust 1.88+ and [mise](https://mise.jdx.dev/).

```bash
mise run setup-models   # first time only
mise run run            # builds and starts the server
```

Or manually:

```bash
cargo build --release
./target/release/openwhisper-cleanup-server --port 8787
```

For grammar checking, you need LanguageTool running separately (default: `http://localhost:8010/v2/check`). Without it, the grammar stage just gets skipped.

## Configuration

CLI flags and matching env vars:

- `--port` / `PORT` - server port (default: 8787)
- `--model-path` - path to ONNX punctuation model (default: `models/pcs_47lang.onnx`)
- `--tokenizer-path` - path to tokenizer (default: `models/tokenizer.json`)
- `--lt-url` / `LT_URL` - LanguageTool API URL (default: `http://localhost:8010/v2/check`)

## Models

The `/v1/models` endpoint lists two models:

- `text-cleanup-pipeline` - cleanup without translation (default)
- `text-cleanup-translate-pl-en` - cleanup + Polish-to-English translation

Translation uses Helsinki-NLP/opus-mt-pl-en running locally via Candle. The model loads on first use, not at startup.

## Custom word replacements

Put a JSON file at `~/.config/openwhisper-cleanup/replacements.json`:

```json
{
  "rules": [
    { "from": "kubernetes", "to": "Kubernetes" },
    { "from": "istio", "to": "Istio", "lang": "en" }
  ]
}
```

The server watches this file and reloads on change.

## OpenWhispr setup

In OpenWhispr settings, add a custom provider:
- Endpoint: `http://localhost:8787/v1`
- Pick a model from the dropdown

The server ignores the system prompt and processes the user message directly through the pipeline.

