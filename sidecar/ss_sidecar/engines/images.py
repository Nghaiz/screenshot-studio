"""Read one image from disk for the real engines, mapping every failure to a per-item error."""
from __future__ import annotations

from pathlib import Path

from .. import config
from ..errors import ItemError


def check_path(path: str) -> Path:
    """Reject what no engine can read before touching the file: unsupported extension, missing file."""
    p = Path(path)
    if p.suffix.lower() not in config.SUPPORTED_EXTENSIONS:
        raise ItemError("unsupported", f"format not supported: {p.suffix or '(no extension)'}")
    if not p.is_file():
        raise ItemError("unreadable", "file not found")
    return p


def open_rgb(path: str):
    """PIL RGB image (EXIF orientation applied), or ItemError."""
    from PIL import Image, ImageOps, UnidentifiedImageError

    p = check_path(path)
    try:
        with Image.open(p) as im:
            im = ImageOps.exif_transpose(im)
            return im.convert("RGB")
    except UnidentifiedImageError as e:
        raise ItemError("unreadable", "cannot decode image") from e
    except Image.DecompressionBombError as e:
        raise ItemError("unsupported", f"image too large: {e}") from e
    except OSError as e:
        raise ItemError("unreadable", f"cannot read image: {e}") from e
