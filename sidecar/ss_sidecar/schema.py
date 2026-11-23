"""Params validation against the contract schema file (the SSOT; no hand-copied shapes in the sidecar)."""
from __future__ import annotations

import json
from pathlib import Path

from jsonschema import Draft202012Validator
from jsonschema.exceptions import best_match

from . import config
from .errors import INTERNAL, INVALID_PARAMS, RpcError


def def_name(method: str, suffix: str) -> str:
    """`classify.zeroShot` + `Params` -> `ClassifyZeroShotParams` (the schema's `$defs` naming)."""
    return "".join(part[:1].upper() + part[1:] for part in method.split(".")) + suffix


class ContractSchema:
    def __init__(self, path: Path | None = None):
        self.path = path or config.schema_path()
        self.error: str | None = None
        self._validators: dict[str, Draft202012Validator] = {}
        try:
            doc = json.loads(self.path.read_text(encoding="utf-8"))
        except (OSError, ValueError) as e:
            self.error = f"contract schema not loadable at {self.path} (set {config.ENV_SCHEMA}): {e}"
            return
        self._defs = doc["$defs"]

    def validator(self, name: str) -> Draft202012Validator:
        if name not in self._validators:
            self._validators[name] = Draft202012Validator({"$ref": f"#/$defs/{name}", "$defs": self._defs})
        return self._validators[name]

    def check(self, def_ref: str, params: object) -> None:
        """Raise -32602 naming the first violation of `$defs/<def_ref>`; -32000 if the schema never loaded."""
        if self.error:
            raise RpcError(INTERNAL, self.error)
        error = best_match(self.validator(def_ref).iter_errors(params))
        if error is not None:
            where = "/".join(str(p) for p in error.absolute_path) or "params"
            raise RpcError(INVALID_PARAMS, f"{where}: {error.message}")
