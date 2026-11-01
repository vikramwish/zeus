#!/usr/bin/env python3
"""External educational contract verifier; run with the candidate as cwd."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    candidate = Path.cwd().resolve()
    program = candidate / "kv.py"
    if not program.is_file() or program.is_symlink():
        print("FAIL: candidate must contain a regular kv.py", file=sys.stderr)
        return 1
    failures = []
    checks = 0
    with tempfile.TemporaryDirectory(prefix=".kv-verify-", dir=str(candidate)) as directory:
        state = Path(directory) / "state.json"

        def check(label, args, status, expected):
            nonlocal checks
            checks += 1
            try:
                result = subprocess.run(
                    [sys.executable, "-I", str(program), str(state)] + args,
                    cwd=str(candidate), stdin=subprocess.DEVNULL,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    timeout=5, check=False,
                )
                actual = json.loads(result.stdout.decode("utf-8"))
                if (result.returncode != status or actual != expected or result.stderr):
                    failures.append("{}: status={}, stdout={!r}, stderr={!r}".format(
                        label, result.returncode, result.stdout[:300], result.stderr[:300]))
            except (OSError, ValueError, subprocess.TimeoutExpired) as error:
                failures.append("{}: {}".format(label, error))

        def unchanged(label, before):
            nonlocal checks
            checks += 1
            after = state.read_bytes() if state.exists() else None
            if after != before:
                failures.append(label + ": state changed")

        missing = {"error": "missing key"}
        invalid = {"error": "invalid input"}
        check("missing get", ["get", "absent"], 1, missing)
        check("missing delete", ["delete", "absent"], 1, missing)
        unchanged("reads do not create state", None)
        for args in ([], ["wat", "x"], ["get"], ["get", "x", "extra"],
                     ["set", "x"], ["set", "x", "{"],
                     ["set", "x", "NaN"], ["set", "x", "Infinity"],
                     ["set", "x", "1e999"], ["delete", "x", "extra"]):
            check("invalid " + repr(args), args, 2, invalid)
            unchanged("invalid input does not create state", None)

        values = ["snow 雪 / café / 😀\nsecond line", None, False, 0,
                  {"nested": [1, True, None, "東京"]}, ""]
        for index, value in enumerate(values):
            key = "ключ-" + str(index)
            check("set " + key, ["set", key, json.dumps(value, ensure_ascii=False)],
                  0, {"ok": True})
            check("persistent get " + key, ["get", key], 0, {"value": value})
        check("overwrite", ["set", "ключ-0", '"replacement"'], 0, {"ok": True})
        check("read overwrite", ["get", "ключ-0"], 0, {"value": "replacement"})
        check("empty key", ["set", "", '"empty key value"'], 0, {"ok": True})
        check("read empty key", ["get", ""], 0, {"value": "empty key value"})

        # A fresh seeded store isolates deletion from the intentional save bug.
        state.write_text(json.dumps({"keep": {"a": 1}, "remove": "old"}), encoding="utf-8")
        check("seeded read", ["get", "keep"], 0, {"value": {"a": 1}})
        check("delete existing", ["delete", "remove"], 0, {"ok": True})
        check("delete persists", ["get", "remove"], 1, missing)
        check("delete preserves other keys", ["get", "keep"], 0, {"value": {"a": 1}})
        before = state.read_bytes()
        check("bad set preserves store", ["set", "keep", "{"], 2, invalid)
        unchanged("bad set preserves bytes", before)
        check("missing delete preserves store", ["delete", "absent"], 1, missing)
        unchanged("missing delete preserves bytes", before)

        for corrupt in (b"{", b"[]", b'{"a": NaN}', b'{"a": 1e999}', b'\xff'):
            state.write_bytes(corrupt)
            for operation in (["get", "a"], ["set", "a", "1"], ["delete", "a"]):
                check("corrupt state " + repr(operation), operation, 2, {"error": "invalid state"})
                unchanged("corrupt state preserved", corrupt)
        # A directory is an invalid readable store on every supported host.
        state.unlink()
        state.mkdir()
        check("state read failure", ["get", "a"], 2, {"error": "invalid state"})
        state.rmdir()

        # Deny both in-place and atomic replacement writes, preserving a readable
        # existing store. Run as an unprivileged POSIX user (not root).
        before = b'{"a": 1}'
        state.write_bytes(before)
        try:
            state.chmod(0o444)
            Path(directory).chmod(0o555)
            for operation in (["set", "a", "2"], ["delete", "a"]):
                check("state write failure " + repr(operation), operation,
                      2, {"error": "invalid state"})
                unchanged("failed write preserves bytes", before)
        finally:
            Path(directory).chmod(0o700)
            state.chmod(0o600)
    for failure in failures:
        print("FAIL: " + failure, file=sys.stderr)
    print("{}: {} checks, {} failures".format("FAIL" if failures else "PASS", checks, len(failures)))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
