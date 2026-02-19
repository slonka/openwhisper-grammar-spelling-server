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

## Benchmark

20 test sentences (mixed Polish/English), 3 measured rounds, 1 warmup round.

```
 Sentence (truncated)                      Lang   Mean ms
 ──────────────────────────────────────────────────────────
 yyy no więc jakby to jest ważne           pl        53.6
 eee znaczy generalnie chciałem powiedzie  pl        47.9
 napewno to jest na prawdę ważne           pl        47.0
 wogóle nie wiem co powiedzieć narazie     pl        46.1
 poprostu przedewszystkim trzeba to zrobi  pl        44.3
 dla tego po mimo wszystko udało się       pl        44.8
 po nie waż to jest na przeciwko           pl        44.5
 mam dwadzieścia trzy lata i mieszkam tu   pl        45.1
 yyy no więc jakby generalnie chciałem po  pl        45.4
 dzień dobry                               pl        43.6
 proszę o pomoc w tej sprawie              pl        45.1
 um like you know basically its fine       en        13.1
 uh i mean sort of actually right          en         8.6
 your going to loose alot                  en        13.3
 its going to effect the weather or not w  en        14.3
 there going to be better then us          en        11.8
 i would of done it if its possible        en        13.9
 i have twenty three cats and five dogs    en        11.8
 um like you know basically your going to  en        15.5
 hello world                               en        30.9

 Overall latency (ms)
 ─────────────────────
 Min:        7.5
 Max:       65.6
 Mean:      32.0
 P50:       43.7
 P95:       57.5
 P99:       65.6
 Total:     1.9s
```

Run `python bench.py` to reproduce (server must be running). See `python bench.py --help` for options.

## Auto-start on macOS login

Register as a launchd user agent so the server starts automatically on login:

```bash
make launchd-install
```

This copies a plist to `~/Library/LaunchAgents/` and loads it. The server will appear under "Open at Login" in macOS Settings > Login Items. Logs go to `/tmp/openwhisper-cleanup-server.log`.

To remove:

```bash
make launchd-uninstall
```
