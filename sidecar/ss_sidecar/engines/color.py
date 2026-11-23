"""ColorExtractor on Pillow: nearest-neighbour downscale (keeps screenshot flat colours unblended), median-cut palette."""
from __future__ import annotations

from .. import config
from .images import open_rgb


def to_hex(rgb) -> str:
    return "#{:02X}{:02X}{:02X}".format(*rgb)


class PillowColorExtractor:
    def __init__(self) -> None:
        from PIL import Image

        self._image = Image

    def dominant(self, path: str) -> tuple[str, list[str]]:
        image = open_rgb(path)
        scale = config.COLOR_SAMPLE_EDGE / max(image.size)
        if scale < 1:
            size = (max(1, round(image.width * scale)), max(1, round(image.height * scale)))
            image = image.resize(size, self._image.Resampling.NEAREST)
        quantized = image.quantize(colors=config.PALETTE_SIZE, method=self._image.Quantize.MEDIANCUT)
        flat = quantized.getpalette() or []
        palette: list[str] = []
        for _count, index in sorted(quantized.getcolors() or [], reverse=True):
            colour = to_hex(flat[index * 3 : index * 3 + 3])
            if colour not in palette:
                palette.append(colour)
        return palette[0], palette
