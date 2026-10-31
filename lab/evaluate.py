#!/usr/bin/python3
"""Fixed-entrypoint sealed evaluator. No providers, database secrets or orders."""
import sys
from evaluator.run import main

if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # Error text is limited to local schema/gate reason, never record bodies,
        # credential values, paths or an exception trace from raw input.
        print("sealed evaluation BLOCKED: " + type(error).__name__, file=sys.stderr)
        sys.exit(1)
