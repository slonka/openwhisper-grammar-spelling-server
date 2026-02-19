import re
import logging
import time
import uuid

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("cleanup-server")

# ---------------------------------------------------------------------------
# Optional imports - gracefully degrade if unavailable
# ---------------------------------------------------------------------------

try:
    from langdetect import detect as langdetect_detect
except ImportError:
    langdetect_detect = None
    logger.warning("langdetect not installed - defaulting to Polish")

try:
    from text_to_num import text2num
except ImportError:
    text2num = None
    logger.warning("text2num not installed - English ITN disabled")

try:
    from itn.pl import NormalizerPL
except ImportError:
    NormalizerPL = None
    logger.warning("pl-itn not installed - Polish ITN disabled")

try:
    from punctuators.models import PunctCapSegModelONNX
except ImportError:
    PunctCapSegModelONNX = None
    logger.warning("punctuators not installed - punctuation restoration disabled")

try:
    import language_tool_python
except ImportError:
    language_tool_python = None
    logger.warning("language_tool_python not installed - grammar correction disabled")


# ---------------------------------------------------------------------------
# Filler patterns
# ---------------------------------------------------------------------------

PL_FILLERS = re.compile(
    r"\b(?:no więc|no właśnie|no wiesz|no nie|no tak"
    r"|tak jakby|w zasadzie"
    r"|yyy+|eee+|hmm+|hm+"
    r"|no|znaczy|jakby|wiesz|powiedzmy|generalnie|kurczę)\b",
    re.IGNORECASE,
)

EN_FILLERS = re.compile(
    r"\b(?:um+|uh+|hmm+|hm+|like|you know|basically|actually"
    r"|literally|i mean|sort of|kind of|right)\b",
    re.IGNORECASE,
)

# ---------------------------------------------------------------------------
# English ITN - number words to digits
# ---------------------------------------------------------------------------

EN_NUMBER_PATTERN = re.compile(
    r"\b("
    r"(?:(?:zero|one|two|three|four|five|six|seven|eight|nine|ten"
    r"|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen"
    r"|eighteen|nineteen|twenty|thirty|forty|fifty|sixty|seventy"
    r"|eighty|ninety|hundred|thousand|million|billion|trillion"
    r"|and)\s*)+"
    r")\b",
    re.IGNORECASE,
)


def _en_itn(text: str) -> str:
    """Replace English number words with digits using text2num."""
    if text2num is None:
        return text

    def _replace(match: re.Match) -> str:
        raw = match.group(0)
        fragment = raw.strip()
        try:
            result = str(text2num(fragment, "en"))
            # Preserve trailing whitespace that the regex consumed
            if raw != fragment and raw.endswith(" "):
                result += " "
            return result
        except Exception:
            return raw

    return EN_NUMBER_PATTERN.sub(_replace, text)


# ---------------------------------------------------------------------------
# Pipeline components (initialized at startup)
# ---------------------------------------------------------------------------

punct_model = None
pl_normalizer = None
lt_pl = None
lt_en = None


app = FastAPI()


@app.on_event("startup")
async def startup():
    global punct_model, pl_normalizer, lt_pl, lt_en

    # Punctuation / capitalization model
    if PunctCapSegModelONNX is not None:
        try:
            logger.info("Loading punctuation model...")
            punct_model = PunctCapSegModelONNX.from_pretrained(
                "pcs_47lang"
            )
            logger.info("Punctuation model loaded.")
        except Exception as e:
            logger.error("Failed to load punctuation model: %s", e)

    # Polish ITN
    if NormalizerPL is not None:
        try:
            logger.info("Initializing Polish ITN normalizer...")
            pl_normalizer = NormalizerPL()
            logger.info("Polish ITN normalizer ready.")
        except Exception as e:
            logger.error("Failed to initialize Polish ITN: %s", e)

    # LanguageTool instances
    if language_tool_python is not None:
        try:
            logger.info("Starting LanguageTool (pl-PL)...")
            lt_pl = language_tool_python.LanguageTool("pl-PL")
            logger.info("LanguageTool pl-PL ready.")
        except Exception as e:
            logger.error("Failed to start LanguageTool pl-PL: %s", e)
        try:
            logger.info("Starting LanguageTool (en-US)...")
            lt_en = language_tool_python.LanguageTool("en-US")
            logger.info("LanguageTool en-US ready.")
        except Exception as e:
            logger.error("Failed to start LanguageTool en-US: %s", e)


# ---------------------------------------------------------------------------
# Pipeline
# ---------------------------------------------------------------------------


def detect_language(text: str) -> str:
    """Return 'pl' or 'en' (default 'pl' on failure)."""
    if langdetect_detect is None:
        return "pl"
    try:
        lang = langdetect_detect(text)
        return "en" if lang.startswith("en") else "pl"
    except Exception:
        return "pl"


def remove_fillers(text: str, lang: str) -> str:
    pattern = PL_FILLERS if lang == "pl" else EN_FILLERS
    text = pattern.sub("", text)
    # Collapse multiple spaces left by removal
    text = re.sub(r"  +", " ", text).strip()
    return text


def inverse_text_normalize(text: str, lang: str) -> str:
    if lang == "pl" and pl_normalizer is not None:
        try:
            return pl_normalizer.normalize(text)
        except Exception as e:
            logger.warning("Polish ITN failed: %s", e)
            return text
    elif lang == "en":
        return _en_itn(text)
    return text


def restore_punctuation(text: str) -> str:
    if punct_model is None:
        return text
    try:
        results = punct_model.infer([text])
        # Returns list of list of segments; join them
        if results and results[0]:
            return " ".join(results[0])
    except Exception as e:
        logger.warning("Punctuation restoration failed: %s", e)
    return text


def correct_grammar(text: str, lang: str) -> str:
    tool = lt_pl if lang == "pl" else lt_en
    if tool is None:
        return text
    try:
        matches = tool.check(text)
        return language_tool_python.utils.correct(text, matches)
    except Exception as e:
        logger.warning("Grammar correction failed: %s", e)
    return text


def run_pipeline(text: str) -> str:
    if not text or not text.strip():
        return text

    logger.info("Input:    %r", text)

    # 1. Language detection
    lang = detect_language(text)
    logger.info("Language: %s", lang)

    # 2. Filler removal
    text = remove_fillers(text, lang)
    logger.info("Fillers:  %r", text)

    # 3. Inverse text normalization
    text = inverse_text_normalize(text, lang)
    logger.info("ITN:      %r", text)

    # 4. Punctuation & capitalization
    text = restore_punctuation(text)
    logger.info("Punct:    %r", text)

    # 5. Grammar correction
    text = correct_grammar(text, lang)
    logger.info("Grammar:  %r", text)

    return text


# ---------------------------------------------------------------------------
# Endpoints
# ---------------------------------------------------------------------------


@app.post("/v1/responses")
async def responses():
    """Return 404 so OpenWhispr falls back to /v1/chat/completions."""
    return JSONResponse(status_code=404, content={"error": "Not found"})


@app.post("/v1/chat/completions")
async def chat_completions(request: Request):
    body = await request.json()
    messages = body.get("messages", [])

    # Extract user message (last message with role=user)
    user_text = ""
    for msg in reversed(messages):
        if msg.get("role") == "user":
            user_text = msg.get("content", "")
            break

    cleaned = run_pipeline(user_text)

    return {
        "id": f"chatcmpl-{uuid.uuid4().hex[:12]}",
        "object": "chat.completion",
        "created": int(time.time()),
        "model": "text-cleanup-pipeline",
        "choices": [
            {
                "index": 0,
                "message": {"role": "assistant", "content": cleaned},
                "finish_reason": "stop",
            }
        ],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
    }


@app.get("/v1/models")
async def list_models():
    return {
        "object": "list",
        "data": [
            {
                "id": "text-cleanup-pipeline",
                "object": "model",
                "created": 0,
                "owned_by": "local",
            }
        ],
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8787)
