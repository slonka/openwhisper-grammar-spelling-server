import json
import re
import logging
import logging.handlers
import time
import uuid
from pathlib import Path
from typing import NamedTuple

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("cleanup-server")

_LOG_DIR = Path.home() / "Library" / "Logs" / "openwhisper-cleanup"
_LOG_DIR.mkdir(parents=True, exist_ok=True)
_file_handler = logging.handlers.TimedRotatingFileHandler(
    _LOG_DIR / "server.log",
    when="W0",  # rotate weekly on Monday
    backupCount=4,
    encoding="utf-8",
)
_file_handler.setFormatter(logging.Formatter("%(asctime)s %(levelname)s:%(name)s:%(message)s"))
logging.getLogger().addHandler(_file_handler)

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
# Context-triggered word correction rules
# ---------------------------------------------------------------------------


class WordCorrectionRule(NamedTuple):
    pattern: re.Pattern
    replacement: str
    description: str


def _compile_rules(raw_rules):
    """Compile raw rule dicts into WordCorrectionRule tuples."""
    return [
        WordCorrectionRule(
            pattern=re.compile(r["pattern"], re.IGNORECASE),
            replacement=r["replacement"],
            description=r["description"],
        )
        for r in raw_rules
    ]


_PL_WORD_CORRECTION_RULES_RAW = [
    # Joins: STT splits compound words
    {"pattern": r"\bna\s+prawdę\b", "replacement": "naprawdę",
     "description": "na prawdę -> naprawdę"},
    {"pattern": r"\bna\s+przeciwko\b", "replacement": "naprzeciwko",
     "description": "na przeciwko -> naprzeciwko"},
    {"pattern": r"\bpo\s+nad\s+to\b", "replacement": "ponadto",
     "description": "po nad to -> ponadto"},
    {"pattern": r"\bpo\s+mimo\b", "replacement": "pomimo",
     "description": "po mimo -> pomimo"},
    {"pattern": r"\bdla\s+tego\b", "replacement": "dlatego",
     "description": "dla tego -> dlatego"},
    {"pattern": r"\bpo\s+nie\s+waż\b", "replacement": "ponieważ",
     "description": "po nie waż -> ponieważ"},
    # Splits: STT joins separate words
    {"pattern": r"\bnapewno\b", "replacement": "na pewno",
     "description": "napewno -> na pewno"},
    {"pattern": r"\bwogóle\b", "replacement": "w ogóle",
     "description": "wogóle -> w ogóle"},
    {"pattern": r"\bnarazie\b", "replacement": "na razie",
     "description": "narazie -> na razie"},
    {"pattern": r"\bconajmniej\b", "replacement": "co najmniej",
     "description": "conajmniej -> co najmniej"},
    {"pattern": r"\bpoprostu\b", "replacement": "po prostu",
     "description": "poprostu -> po prostu"},
    {"pattern": r"\bprzedewszystkim\b", "replacement": "przede wszystkim",
     "description": "przedewszystkim -> przede wszystkim"},
]

_EN_WORD_CORRECTION_RULES_RAW = [
    # your + verb/adj -> you're
    {"pattern": r"\byour\b(?=\s+(?:going|doing|being|making|getting|coming"
                r"|running|saying|looking|trying|giving|taking|having"
                r"|welcome|not|right|wrong))",
     "replacement": "you're",
     "description": "your + verb -> you're"},
    # its + verb/adv -> it's
    {"pattern": r"\bits\b(?=\s+(?:going|doing|being|getting|making|coming"
                r"|running|not|been|just|about|really|very|always|never"
                r"|still|already|only|also|a\b|the\b|so\b|ok\b|okay\b"
                r"|true\b|possible\b|impossible\b|important\b))",
     "replacement": "it's",
     "description": "its + verb/adv -> it's"},
    # there + verb -> they're
    {"pattern": r"\bthere\b(?=\s+(?:going|doing|being|making|getting|coming"
                r"|running|saying|looking|trying|giving|playing|telling"
                r"|leaving|taking|having|showing|not|always|never|just"
                r"|really|still|already))",
     "replacement": "they're",
     "description": "there + verb -> they're"},
    # their + verb -> they're
    {"pattern": r"\btheir\b(?=\s+(?:going|doing|being|making|getting|coming"
                r"|running|saying|looking|trying|giving|playing|telling"
                r"|leaving|taking|having|showing|not|always|never|just"
                r"|really|still|already))",
     "replacement": "they're",
     "description": "their + verb -> they're"},
    # whose + verb -> who's
    {"pattern": r"\bwhose\b(?=\s+(?:going|doing|being|making|getting|coming"
                r"|running|not|been|there|here))",
     "replacement": "who's",
     "description": "whose + verb -> who's"},
    # weather + clause -> whether
    {"pattern": r"\bweather\b(?=\s+(?:or|it|you|we|they|he|she|to\b|not\b))",
     "replacement": "whether",
     "description": "weather + clause -> whether"},
    # comparative + then -> than
    {"pattern": r"\b(more|less|better|worse|bigger|smaller|larger|higher"
                r"|lower|faster|slower|older|younger|harder|easier"
                r"|rather|other)\s+then\b",
     "replacement": r"\1 than",
     "description": "comparative + then -> than"},
    # article/adj + affect -> effect
    {"pattern": r"\b(the|an?|no|any|this|that|its|positive|negative)\s+affect\b",
     "replacement": r"\1 effect",
     "description": "article + affect -> effect"},
    # modal/to + effect -> affect
    {"pattern": r"\b(will|would|could|can|may|might|to|not)\s+effect\b",
     "replacement": r"\1 affect",
     "description": "modal + effect -> affect"},
    # verb context + loose -> lose
    {"pattern": r"\b(to|will|would|could|gonna|might|can|don't|didn't"
                r"|won't|cannot)\s+loose\b",
     "replacement": r"\1 lose",
     "description": "verb context + loose -> lose"},
    # modal + of -> have
    {"pattern": r"\b(would|could|should|might|must)\s+of\b",
     "replacement": r"\1 have",
     "description": "modal + of -> have"},
    # alot -> a lot
    {"pattern": r"\balot\b", "replacement": "a lot",
     "description": "alot -> a lot"},
    # copula + to + adj -> too
    {"pattern": r"\b(is|are|was|were|am|be|been)\s+to\b(?=\s+(?:big|small"
                r"|large|much|many|few|little|hard|easy|late|early|fast"
                r"|slow|long|short|hot|cold|old|young|good|bad|high|low"
                r"|far|close|loud|quiet|expensive|cheap|difficult|simple))",
     "replacement": r"\1 too",
     "description": "copula + to + adj -> too"},
]

PL_WORD_CORRECTION_RULES = _compile_rules(_PL_WORD_CORRECTION_RULES_RAW)
EN_WORD_CORRECTION_RULES = _compile_rules(_EN_WORD_CORRECTION_RULES_RAW)

# ---------------------------------------------------------------------------
# User-defined word replacements from config file
# ---------------------------------------------------------------------------

_USER_REPLACEMENTS_PATH = (
    Path.home() / ".config" / "openwhisper-cleanup" / "replacements.json"
)


def _load_user_replacements():
    """Load user-defined replacement rules from config file.

    Returns a list of (rule, lang_filter) tuples where lang_filter is
    None (apply to both), "pl", or "en".
    """
    if not _USER_REPLACEMENTS_PATH.is_file():
        return []

    try:
        data = json.loads(_USER_REPLACEMENTS_PATH.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as e:
        logger.warning("Failed to read user replacements file: %s", e)
        return []

    if isinstance(data, dict):
        data = data.get("rules", [])
    elif not isinstance(data, list):
        logger.warning("User replacements file: expected object or array at top level")
        return []

    rules = []
    for i, entry in enumerate(data):
        if not isinstance(entry, dict):
            logger.warning("User replacement #%d: expected object, skipping", i)
            continue
        from_text = entry.get("from")
        to_text = entry.get("to")
        if not from_text or not isinstance(from_text, str):
            logger.warning("User replacement #%d: missing/invalid 'from', skipping", i)
            continue
        if to_text is None or not isinstance(to_text, str):
            logger.warning("User replacement #%d: missing/invalid 'to', skipping", i)
            continue

        lang_filter = entry.get("lang")
        if lang_filter is not None and lang_filter not in ("pl", "en"):
            logger.warning(
                "User replacement #%d: invalid lang %r, ignoring filter", i, lang_filter
            )
            lang_filter = None

        pattern = re.compile(r"\b" + re.escape(from_text) + r"\b", re.IGNORECASE)
        rule = WordCorrectionRule(
            pattern=pattern,
            replacement=to_text,
            description=f"{from_text} -> {to_text}",
        )
        rules.append((rule, lang_filter))

    logger.info("Loaded %d user replacement rule(s) from %s", len(rules), _USER_REPLACEMENTS_PATH)
    return rules


_user_replacements_rules = _load_user_replacements()
_user_replacements_mtime: float | None = None
try:
    _user_replacements_mtime = _USER_REPLACEMENTS_PATH.stat().st_mtime
except OSError:
    pass


def _get_user_replacements():
    """Return current user replacement rules, reloading if the config file changed."""
    global _user_replacements_rules, _user_replacements_mtime
    try:
        mtime = _USER_REPLACEMENTS_PATH.stat().st_mtime
    except OSError:
        # File doesn't exist (or can't be read)
        if _user_replacements_mtime is not None:
            logger.info("User replacements file removed, clearing rules")
            _user_replacements_rules = []
            _user_replacements_mtime = None
        return _user_replacements_rules

    if mtime != _user_replacements_mtime:
        logger.info("User replacements file changed, reloading")
        _user_replacements_rules = _load_user_replacements()
        _user_replacements_mtime = mtime

    return _user_replacements_rules

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


_BACKREF_RE = re.compile(r"\\[0-9]")


def _preserve_case_replacement(replacement):
    """Return a re.sub replacement function that preserves the case of the match."""
    def _replacer(match):
        original = match.group(0)
        if original.isupper():
            return replacement.upper()
        if original[0].isupper():
            return replacement[0].upper() + replacement[1:]
        return replacement
    return _replacer


def apply_word_corrections(text: str, lang: str) -> str:
    """Apply context-triggered word corrections for the given language."""
    try:
        rules = PL_WORD_CORRECTION_RULES if lang == "pl" else EN_WORD_CORRECTION_RULES
        for rule in rules:
            if _BACKREF_RE.search(rule.replacement):
                new_text = rule.pattern.sub(rule.replacement, text)
            else:
                new_text = rule.pattern.sub(
                    _preserve_case_replacement(rule.replacement), text
                )
            if new_text != text:
                logger.debug("Word correction: %s", rule.description)
                text = new_text
        return text
    except Exception as e:
        logger.warning("Word correction failed: %s", e)
        return text


def apply_user_replacements(text: str, lang: str) -> str:
    """Apply user-defined word replacements from config file."""
    rules = _get_user_replacements()
    if not rules:
        return text
    try:
        for rule, lang_filter in rules:
            if lang_filter is not None and lang_filter != lang:
                continue
            if _BACKREF_RE.search(rule.replacement):
                new_text = rule.pattern.sub(rule.replacement, text)
            else:
                new_text = rule.pattern.sub(
                    _preserve_case_replacement(rule.replacement), text
                )
            if new_text != text:
                logger.debug("User replacement: %s", rule.description)
                text = new_text
        return text
    except Exception as e:
        logger.warning("User replacement failed: %s", e)
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

    # 4.5 Context-triggered word corrections
    text = apply_word_corrections(text, lang)
    logger.info("Words:    %r", text)

    # 4.7 User-defined replacements
    text = apply_user_replacements(text, lang)
    logger.info("User:     %r", text)

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
    import os
    import uvicorn

    port = int(os.environ.get("PORT", "8787"))
    uvicorn.run(app, host="0.0.0.0", port=port)
