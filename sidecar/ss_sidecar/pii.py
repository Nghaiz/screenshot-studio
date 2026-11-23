"""PII finder over OCR lines (contract `pii.detect`): email, phone (VN + intl), URL, card (Luhn). Stdlib only.

A finding's box is the line box narrowed to the match by character position (OCR gives no per-character boxes).
Kinds are tried in PRIORITY order and a later kind never reports a span an earlier kind already took, so the
digits of a card are not also a phone and the host of an email is not also a URL.
"""
from __future__ import annotations

import re

EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")
URL = re.compile(r"(?:https?://|www\.)[^\s<>\"'`]+", re.IGNORECASE)
CARD = re.compile(r"(?<!\d)\d(?:[ -]?\d){12,18}(?!\d)")
PHONE = re.compile(r"(?<![\w+])(?:\+|00)?\(?\d[\d ().-]{7,20}\d(?!\w)")

URL_TRAILING = ".,;:!?)]}'\""
# Vietnamese domestic numbers: mobile 0[35789] + 8 digits; landline 02 + 9 digits.
VN_DOMESTIC = re.compile(r"0[35789]\d{8}|02\d{9}")
INTL_DIGITS = (8, 15)  # E.164 allows up to 15 digits after the country-code prefix
CARD_DIGITS = (13, 19)


def luhn_ok(digits: str) -> bool:
    total = 0
    for i, ch in enumerate(reversed(digits)):
        d = int(ch)
        if i % 2 == 1:
            d = d * 2 - 9 if d > 4 else d * 2
        total += d
    return total % 10 == 0


def _digits(s: str) -> str:
    return re.sub(r"\D", "", s)


def _email(m: re.Match) -> str | None:
    return m.group(0)


def _url(m: re.Match) -> str | None:
    text = m.group(0).rstrip(URL_TRAILING)
    return text if len(text) > len("www.") else None


def _card(m: re.Match) -> str | None:
    digits = _digits(m.group(0))
    return m.group(0) if CARD_DIGITS[0] <= len(digits) <= CARD_DIGITS[1] and luhn_ok(digits) else None


def _phone(m: re.Match) -> str | None:
    text = m.group(0)
    digits = _digits(text)
    if text.startswith("+") or text.startswith("00"):
        national = digits[2:] if text.startswith("00") else digits
        return text if INTL_DIGITS[0] <= len(national) <= INTL_DIGITS[1] else None
    if VN_DOMESTIC.fullmatch(digits):
        return text
    return None


PRIORITY = (("email", EMAIL, _email), ("url", URL, _url), ("card", CARD, _card), ("phone", PHONE, _phone))


def _sub_box(box: list[float], start: int, end: int, length: int) -> list[float]:
    x1, y1, x2, y2 = (float(v) for v in box)
    width = x2 - x1
    return [round(x1 + width * start / length, 1), y1, round(x1 + width * end / length, 1), y2]


def find_spans(text: str) -> list[tuple[str, int, int, str]]:
    """(kind, start, end, matched text) for one line, no two spans overlapping."""
    taken: list[tuple[int, int]] = []
    spans = []
    for kind, pattern, accept in PRIORITY:
        for m in pattern.finditer(text):
            found = accept(m)
            if not found:
                continue
            start, end = m.start(), m.start() + len(found)
            if any(start < t_end and t_start < end for t_start, t_end in taken):
                continue
            taken.append((start, end))
            spans.append((kind, start, end, found))
    return sorted(spans, key=lambda s: s[1])


def detect(lines: list[dict]) -> list[dict]:
    findings = []
    for line in lines:
        text = line["text"]
        for kind, start, end, found in find_spans(text):
            findings.append({"kind": kind, "text": found, "box": _sub_box(line["box"], start, end, len(text))})
    return findings
