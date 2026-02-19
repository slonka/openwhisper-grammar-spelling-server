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
```

No tests or linting configured.

## Architecture

Single-file server (`server.py`) with a 5-stage text cleanup pipeline in `run_pipeline()`:

1. **Language detection** - `langdetect` to classify as Polish or English (defaults to Polish)
2. **Filler removal** - regex-based removal of speech fillers ("yyy", "um", "jakby", etc.)
3. **Inverse text normalization (ITN)** - converts number words to digits. Polish uses `pl-itn` (`NormalizerPL`), English uses `text2num`
4. **Punctuation/capitalization** - `punctuators` ONNX model (`pcs_47lang`)
5. **Grammar correction** - `language_tool_python` (separate instances for pl-PL and en-US)

All dependencies are optional - each import is wrapped in try/except and the pipeline gracefully skips unavailable stages.

## API Endpoints

- `POST /v1/chat/completions` - main endpoint; extracts last user message, runs pipeline, returns OpenAI-compatible response
- `POST /v1/responses` - returns 404 to force OpenWhispr to fall back to `/v1/chat/completions`
- `GET /v1/models` - lists the single `text-cleanup-pipeline` model

## Key Detail

The `requirements.txt` is missing `pl-itn` (provides `itn.pl.NormalizerPL` for Polish ITN). It's an optional dependency handled gracefully at runtime.
