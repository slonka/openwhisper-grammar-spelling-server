# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A FastAPI server that post-processes speech-to-text output from OpenWhispr. It exposes OpenAI-compatible endpoints so OpenWhispr can send raw transcription text and receive cleaned-up text back. The server is **not** an LLM - it runs a deterministic NLP pipeline.

## Commands

```bash
# Setup
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt

# Run (listens on 0.0.0.0:8787)
python server.py

# Dev (port 9787 - production runs on 8787 via launchd)
make dev

# Benchmark (server must be running)
python bench.py
python bench.py --rounds 5 --warmup 2
```

No tests or linting configured.

## Architecture

Single-file server (`server.py`) with a 6-stage text cleanup pipeline in `run_pipeline()`:

1. **Language detection** - `langdetect` to classify as Polish or English (defaults to Polish)
2. **Filler removal** - regex-based removal of speech fillers ("yyy", "um", "jakby", etc.)
3. **Inverse text normalization (ITN)** - converts number words to digits. Polish uses `pl-itn` (`NormalizerPL`), English uses `text2num`
4. **Punctuation/capitalization** - `punctuators` ONNX model (`pcs_47lang`)
5. **Word corrections** - regex-based context-triggered fixes for compound word splits/joins (Polish) and homophone confusion (English); no extra dependencies
6. **User replacements** - user-defined word/phrase replacements from `~/.config/openwhisper-cleanup/replacements.json`
7. **Grammar correction** - `language_tool_python` (separate instances for pl-PL and en-US)

All dependencies are optional - each import is wrapped in try/except and the pipeline gracefully skips unavailable stages.

## API Endpoints

- `POST /v1/chat/completions` - main endpoint; extracts last user message, runs pipeline, returns OpenAI-compatible response
- `POST /v1/responses` - returns 404 to force OpenWhispr to fall back to `/v1/chat/completions`
- `GET /v1/models` - lists the single `text-cleanup-pipeline` model

## Conventions

- Use `jq` instead of `python3 -m json.tool` for formatting JSON output.

## Configuration

User-defined word replacements can be added in `~/.config/openwhisper-cleanup/replacements.json`. The file contains a JSON object with a `"rules"` key holding an array of objects with `from`, `to`, and optional `lang` (`"pl"` or `"en"`) fields. A bare top-level array is also accepted for backward compatibility. Rules are hot-reloaded on each request when the file's mtime changes - no restart needed.

## Key Detail

The `requirements.txt` is missing `pl-itn` (provides `itn.pl.NormalizerPL` for Polish ITN). It's an optional dependency handled gracefully at runtime.
