#!/usr/bin/env python3
"""Repair exercise: save_state deliberately does not persist mutations."""
import json
import math
from pathlib import Path
import sys


def reject_constant(value):
    raise ValueError("non-finite JSON number")


def finite_float(value):
    number = float(value)
    if not math.isfinite(number):
        raise ValueError("non-finite JSON number")
    return number


def decode(text):
    return json.loads(text, parse_constant=reject_constant, parse_float=finite_float)


def save_state(path, state):
    # INTENTIONAL BUG: mutations vanish at process exit. Implement persistence here.
    pass


def emit(value, status=0):
    print(json.dumps(value, ensure_ascii=True, allow_nan=False))
    return status


def main(args):
    if len(args) < 3:
        return emit({"error": "invalid input"}, 2)
    path, operation, key = Path(args[0]), args[1], args[2]
    if operation not in ("get", "set", "delete") or len(args) != (4 if operation == "set" else 3):
        return emit({"error": "invalid input"}, 2)
    if operation == "set":
        try:
            value = decode(args[3])
        except (ValueError, RecursionError):
            return emit({"error": "invalid input"}, 2)
    try:
        try:
            state = decode(path.read_text(encoding="utf-8"))
        except FileNotFoundError:
            state = {}
        if not isinstance(state, dict):
            raise ValueError("state must be an object")
    except (ValueError, OSError, RecursionError):
        return emit({"error": "invalid state"}, 2)
    if operation == "get":
        if key not in state:
            return emit({"error": "missing key"}, 1)
        return emit({"value": state[key]})
    if operation == "delete":
        if key not in state:
            return emit({"error": "missing key"}, 1)
        del state[key]
    else:
        state[key] = value
    try:
        save_state(path, state)
    except (OSError, ValueError, RecursionError):
        return emit({"error": "invalid state"}, 2)
    return emit({"ok": True})


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
