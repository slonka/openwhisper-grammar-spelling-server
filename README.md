# openwhisper-grammar-spelling-server

A post-processing server for [OpenWhispr](https://github.com/openwhispr) that cleans up speech-to-text output. Exposes OpenAI-compatible API endpoints and runs a pipeline of filler removal, inverse text normalization, punctuation restoration, and grammar correction. Supports Polish and English.

## Building

Requires Python 3.12+, a C compiler, and [Nuitka](https://nuitka.net/).

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
make build
```

The binary is produced at `dist/openwhisper-cleanup-server`.

## Installing

Install the binary to `~/bin`:

```bash
make install
```

Make sure `~/bin` is in your PATH (add to `~/.zshrc` if not):

```bash
export PATH="$HOME/bin:$PATH"
```

To remove:

```bash
make uninstall
```

## Running

```bash
openwhisper-cleanup-server
```

The server listens on `http://0.0.0.0:8787`.

Point OpenWhispr's post-processing URL to `http://localhost:8787/v1/chat/completions`.

## Auto-start on macOS login

Register as a launchd user agent so the server starts automatically on login:

```bash
make launchd-install
```

This copies a plist to `~/Library/LaunchAgents/` and loads it. The server will restart automatically if it crashes (`KeepAlive` is enabled). Logs go to `/tmp/openwhisper-cleanup-server.log`.

To remove:

```bash
make launchd-uninstall
```
