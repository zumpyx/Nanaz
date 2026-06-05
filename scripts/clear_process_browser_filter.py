#!/usr/bin/env python3
import argparse
import json
import os
import ssl
import sys
import urllib.error
import urllib.request


DEFAULT_GRAPHQL_URL = "https://127.0.0.1:7443/graphql/"


def parse_args():
    parser = argparse.ArgumentParser(
        description="Clear saved Mythic Process Browser column filters for the supplied API token."
    )
    parser.add_argument(
        "token",
        nargs="?",
        default=os.environ.get("MYTHIC_TOKEN"),
        help="Mythic API token. Defaults to MYTHIC_TOKEN.",
    )
    parser.add_argument(
        "-u",
        "--url",
        default=os.environ.get("MYTHIC_GRAPHQL_URL", DEFAULT_GRAPHQL_URL),
        help=f"GraphQL URL. Defaults to {DEFAULT_GRAPHQL_URL}.",
    )
    parser.add_argument(
        "--verify-tls",
        action="store_true",
        help="Verify TLS certificates. By default, local Mythic self-signed certificates are accepted.",
    )
    return parser.parse_args()


def clear_filter(url, token, verify_tls):
    payload = {
        "query": (
            "mutation ClearProcessBrowserFilter { "
            "updateOperatorPreferences(preferences: {process_browser_filter_options: {}}) { "
            "status error "
            "} "
            "}"
        )
    }
    body = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        url,
        data=body,
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
        },
        method="POST",
    )

    context = None
    if url.lower().startswith("https://") and not verify_tls:
        context = ssl._create_unverified_context()

    with urllib.request.urlopen(request, context=context, timeout=15) as response:
        return json.loads(response.read().decode("utf-8"))


def main():
    args = parse_args()
    if not args.token:
        print("error: missing Mythic API token; pass it as an argument or set MYTHIC_TOKEN", file=sys.stderr)
        return 1

    try:
        result = clear_filter(args.url, args.token, args.verify_tls)
    except urllib.error.HTTPError as error:
        print(f"error: HTTP {error.code}: {error.read().decode('utf-8', errors='replace')}", file=sys.stderr)
        return 1
    except urllib.error.URLError as error:
        print(f"error: request failed: {error.reason}", file=sys.stderr)
        return 1
    except json.JSONDecodeError as error:
        print(f"error: invalid JSON response: {error}", file=sys.stderr)
        return 1

    print(json.dumps(result, indent=2, sort_keys=True))
    if result.get("errors"):
        return 1
    update_result = result.get("data", {}).get("updateOperatorPreferences", {})
    return 0 if update_result.get("status") == "success" else 1


if __name__ == "__main__":
    raise SystemExit(main())
