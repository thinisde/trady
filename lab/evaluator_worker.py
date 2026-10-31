#!/usr/bin/python3
"""Deployment-only Unix worker; it never downloads, orders or opens a DB."""
import argparse
import pathlib
import sys
from evaluator.transport import serve


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--lock", type=pathlib.Path, required=True)
    parser.add_argument("--socket", type=pathlib.Path, required=True)
    parser.add_argument("--state-directory", type=pathlib.Path, required=True)
    args = parser.parse_args()
    serve(args.lock, args.socket, args.state_directory)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print("sealed evaluator worker BLOCKED: " + type(error).__name__, file=sys.stderr)
        sys.exit(1)
